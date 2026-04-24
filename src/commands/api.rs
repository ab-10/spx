use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::Deserialize;
use std::env;
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

/// Build a multipart/form-data body with a single `code` field containing the archive.
pub fn build_multipart_body(archive: &[u8]) -> (String, Vec<u8>) {
    let boundary = "----spx-upload-boundary";
    let mut body = Vec::new();

    // Part header
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"code\"; filename=\"code.tar.gz\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: application/gzip\r\n");
    body.extend_from_slice(b"\r\n");

    // Part body
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
}

pub fn post_run(api_url: &str, token: &str, archive: &[u8], verbose: bool) -> Result<RunResponse> {
    let url = format!("{}/run", api_url.trim_end_matches('/'));
    if verbose {
        ui::verbose(&format!("POST {url}"));
        ui::verbose(&format!("Archive size: {} bytes", archive.len()));
    }

    let (content_type, body) = build_multipart_body(archive);

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
