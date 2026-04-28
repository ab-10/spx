use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Deserialize;
use std::env;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use crate::ui;

const DEFAULT_API_URL: &str = "https://api.runspx.com";

pub fn api_url() -> String {
    env::var("SPX_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

/// Create a tar.gz archive of a project directory, excluding build artifacts.
pub fn create_archive(dir: &Path) -> Result<Vec<u8>> {
    let buf = Vec::new();
    let encoder = GzEncoder::new(buf, Compression::fast());
    let mut archive = tar::Builder::new(encoder);

    add_dir_recursive(&mut archive, dir, dir)?;

    let encoder = archive.into_inner().context("finalizing tar archive")?;
    encoder.finish().context("finalizing gzip stream")
}

fn add_dir_recursive<W: std::io::Write>(
    archive: &mut tar::Builder<W>,
    root: &Path,
    current: &Path,
) -> Result<()> {
    let entries = std::fs::read_dir(current)
        .with_context(|| format!("reading directory {}", current.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        // Skip symlinks
        if file_type.is_symlink() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let rel_path = path.strip_prefix(root).unwrap();

        if file_type.is_dir() {
            // Exclude __pycache__ at any depth
            if name_str == "__pycache__" {
                continue;
            }
            // Exclude specific dirs at root only
            if current == root
                && (name_str == ".git" || name_str == ".venv" || name_str == ".spx")
            {
                continue;
            }
            archive
                .append_dir(rel_path, &path)
                .with_context(|| format!("adding directory {}", rel_path.display()))?;
            add_dir_recursive(archive, root, &path)?;
        } else if file_type.is_file() {
            archive
                .append_path_with_name(&path, rel_path)
                .with_context(|| format!("adding file {}", rel_path.display()))?;
        }
    }

    Ok(())
}

/// Build a multipart/form-data body with a `code` archive plus an `entry` text field.
pub fn build_multipart_body(archive: &[u8], entry: &str) -> (String, Vec<u8>) {
    let boundary = "----spx-upload-boundary";
    let mut body = Vec::new();

    // entry field
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"entry\"\r\n");
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(entry.as_bytes());
    body.extend_from_slice(b"\r\n");

    // code archive field
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"code\"; filename=\"code.tar.gz\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/gzip\r\n");
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(archive);

    // Closing boundary
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let content_type = format!("multipart/form-data; boundary={boundary}");
    (content_type, body)
}

#[derive(Deserialize)]
pub struct RunResponse {
    pub url: String,
    #[allow(dead_code)]
    pub username: String,
    pub pet_name: String,
}

pub fn post_run(
    api_url: &str,
    token: &str,
    archive: &[u8],
    entry: &str,
    verbose: bool,
) -> Result<RunResponse> {
    let url = format!("{}/run", api_url.trim_end_matches('/'));
    if verbose {
        ui::verbose(&format!("POST {url}"));
        ui::verbose(&format!("Archive size: {} bytes", archive.len()));
        ui::verbose(&format!("Entry: {entry}"));
    }

    let (content_type, body) = build_multipart_body(archive, entry);

    match ureq::post(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", &content_type)
        .send_bytes(&body)
    {
        Ok(resp) => {
            let run_resp: RunResponse = resp.into_json().context("parsing run response")?;
            Ok(run_resp)
        }
        Err(ureq::Error::Status(code, resp)) => {
            if code == 401 || code == 403 {
                bail!(
                    "session invalid or expired. Run `spx login` to re-authenticate."
                );
            }
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some(detail) = parse_error_body(&body) {
                ui::warn(&detail);
                std::process::exit(1);
            }
            bail!("POST {url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("POST {url} failed: {t}"),
    }
}

/// Extract the `detail` field from a JSON error response, if present.
pub fn parse_error_body(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    Some(v.get("detail")?.as_str()?.to_string())
}

/// One parsed SSE event.
#[derive(Debug, Clone, Default)]
pub struct SseEvent {
    pub id: Option<String>,
    pub event: String,
    pub data: String,
}

/// Open an SSE stream to `url`, calling `on_event` for each event.
///
/// Returns once a `terminal` event is seen (caller signals via the closure
/// returning `Ok(true)`), the stream ends, or a transport error occurs.
/// On EOF without a terminal event, reconnects with `Last-Event-ID` until
/// `max_retries` is exhausted.
pub fn stream_sse<F>(
    url: &str,
    token: &str,
    max_retries: u32,
    mut on_event: F,
) -> Result<()>
where
    F: FnMut(&SseEvent) -> Result<bool>,
{
    let mut last_id: Option<String> = None;
    let mut retries: u32 = 0;
    loop {
        let mut req = ureq::get(url)
            .set("Authorization", &format!("Bearer {token}"))
            .set("Accept", "text/event-stream");
        if let Some(id) = &last_id {
            req = req.set("Last-Event-ID", id);
        }

        let resp = match req.call() {
            Ok(r) => r,
            Err(ureq::Error::Status(404, _)) => {
                bail!("deproc not found (404)");
            }
            Err(ureq::Error::Status(403, _)) => {
                bail!("not your deproc (403)");
            }
            Err(ureq::Error::Status(401, _)) => {
                bail!("session invalid or expired. Run `spx login`.");
            }
            Err(ureq::Error::Status(410, _)) => {
                bail!("deproc buffer purged (410 Gone)");
            }
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                bail!("GET {url} returned {code}: {body}");
            }
            Err(ureq::Error::Transport(t)) => {
                if retries >= max_retries {
                    bail!("SSE transport failed after {retries} retries: {t}");
                }
                retries += 1;
                std::thread::sleep(std::time::Duration::from_millis(
                    250 * (1 << retries.min(5)),
                ));
                continue;
            }
        };

        let reader: Box<dyn Read + Send + Sync> = resp.into_reader();
        let buf = BufReader::new(reader);
        let mut current = SseEvent::default();
        let mut got_any = false;
        let mut terminal = false;
        for line_res in buf.lines() {
            let line = match line_res {
                Ok(l) => l,
                Err(_) => break, // transport hiccup → reconnect
            };
            got_any = true;
            if line.is_empty() {
                if !current.event.is_empty() || !current.data.is_empty() {
                    if let Some(id) = &current.id {
                        last_id = Some(id.clone());
                    }
                    if on_event(&current)? {
                        terminal = true;
                        break;
                    }
                }
                current = SseEvent::default();
                continue;
            }
            if line.starts_with(':') {
                continue; // comment / heartbeat
            }
            if let Some(rest) = line.strip_prefix("id:") {
                current.id = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("event:") {
                current.event = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("data:") {
                if !current.data.is_empty() {
                    current.data.push('\n');
                }
                current.data.push_str(rest.trim_start());
            }
        }

        if terminal {
            return Ok(());
        }

        // Stream ended without a terminal event.
        if retries >= max_retries {
            bail!("SSE stream ended before terminal event (after {retries} retries)");
        }
        if !got_any {
            // Bail out on immediately-empty bodies to avoid spinning.
            retries += 1;
            std::thread::sleep(std::time::Duration::from_millis(
                250 * (1 << retries.min(5)),
            ));
        } else {
            retries += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_body_extracts_detail() {
        let body = r#"{"detail":"something went wrong"}"#;
        assert_eq!(parse_error_body(body).unwrap(), "something went wrong");
    }

    #[test]
    fn parse_error_body_invalid_json() {
        assert!(parse_error_body("not json").is_none());
    }

    #[test]
    fn parse_error_body_no_detail() {
        assert!(parse_error_body(r#"{"other": "field"}"#).is_none());
    }

    #[test]
    fn create_archive_excludes_correctly() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Create files and directories
        std::fs::write(root.join("main.py"), "print('hello')").unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        std::fs::write(root.join(".git/config"), "").unwrap();
        std::fs::create_dir(root.join(".venv")).unwrap();
        std::fs::write(root.join(".venv/pyvenv.cfg"), "").unwrap();
        std::fs::create_dir(root.join(".spx")).unwrap();
        std::fs::write(root.join(".spx/state.json"), "").unwrap();
        std::fs::create_dir(root.join("__pycache__")).unwrap();
        std::fs::write(root.join("__pycache__/main.cpython-312.pyc"), "").unwrap();
        std::fs::create_dir_all(root.join("pkg/__pycache__")).unwrap();
        std::fs::write(root.join("pkg/__pycache__/mod.cpython-312.pyc"), "").unwrap();
        std::fs::write(root.join("pkg/mod.py"), "x = 1").unwrap();

        let archive_bytes = create_archive(root).unwrap();
        assert!(!archive_bytes.is_empty());

        // Decompress and check contents
        let decoder = flate2::read::GzDecoder::new(&archive_bytes[..]);
        let mut archive = tar::Archive::new(decoder);
        let paths: Vec<String> = archive
            .entries()
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(paths.iter().any(|p| p == "main.py"), "should include main.py; got: {paths:?}");
        assert!(paths.iter().any(|p| p == "pkg/mod.py"), "should include pkg/mod.py; got: {paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with(".git")), "should exclude .git; got: {paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with(".venv")), "should exclude .venv; got: {paths:?}");
        assert!(!paths.iter().any(|p| p.starts_with(".spx")), "should exclude .spx; got: {paths:?}");
        assert!(!paths.iter().any(|p| p.contains("__pycache__")), "should exclude __pycache__; got: {paths:?}");
    }
}
