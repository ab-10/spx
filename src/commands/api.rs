use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::env;

#[derive(serde::Serialize)]
struct FeedbackPayload<'a, T: serde::Serialize> {
    message: &'a str,
    context: &'a T,
}

const DEFAULT_API_URL: &str = "https://api.runspx.com";
const MAX_FEEDBACK_BYTES: usize = 1024 * 1024;

pub fn api_url() -> String {
    env::var("SPX_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string())
}

pub fn build_pub_multipart_body(file_bytes: &[u8], filename: &str) -> (String, Vec<u8>) {
    let boundary = "----spx-pub-upload-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            filename.replace('"', "_")
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: text/html\r\n");
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct PubResponse {
    pub slug: String,
    pub url: String,
    pub original_filename: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub content_type: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct PubListResponse {
    pub pubs: Vec<PubResponse>,
}

#[derive(Deserialize)]
pub struct BillingCheckoutResponse {
    pub url: String,
}

#[derive(Deserialize)]
pub struct BillingStatusResponse {
    pub has_access: bool,
    #[allow(dead_code)]
    pub status: Option<String>,
    #[allow(dead_code)]
    pub current_period_end: Option<String>,
}

pub fn pub_create(
    api_url: &str,
    token: &str,
    file_bytes: &[u8],
    filename: &str,
) -> Result<PubResponse> {
    let url = format!("{}/pub", api_url.trim_end_matches('/'));
    let (content_type, body) = build_pub_multipart_body(file_bytes, filename);
    match ureq::post(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", &content_type)
        .send_bytes(&body)
    {
        Ok(resp) => resp.into_json().context("parsing pub create response"),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            bail!("session invalid or expired. Run `spx login` to re-authenticate.")
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some(detail) = parse_error_body(&body) {
                bail!("{detail}");
            }
            bail!("POST {url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("POST {url} failed: {t}"),
    }
}

pub fn pub_update(
    api_url: &str,
    token: &str,
    slug_or_url: &str,
    file_bytes: &[u8],
    filename: &str,
) -> Result<PubResponse> {
    let url = format!(
        "{}/pub/{}",
        api_url.trim_end_matches('/'),
        percent_encode_query_value(slug_or_url)
    );
    let (content_type, body) = build_pub_multipart_body(file_bytes, filename);
    match ureq::put(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", &content_type)
        .send_bytes(&body)
    {
        Ok(resp) => resp.into_json().context("parsing pub update response"),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            bail!("session invalid or expired. Run `spx login` to re-authenticate.")
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some(detail) = parse_error_body(&body) {
                bail!("{detail}");
            }
            bail!("PUT {url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("PUT {url} failed: {t}"),
    }
}

pub fn pub_delete(api_url: &str, token: &str, slug_or_url: &str) -> Result<()> {
    let url = format!(
        "{}/pub/{}",
        api_url.trim_end_matches('/'),
        percent_encode_query_value(slug_or_url)
    );
    match ureq::delete(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            bail!("session invalid or expired. Run `spx login` to re-authenticate.")
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some(detail) = parse_error_body(&body) {
                bail!("{detail}");
            }
            bail!("DELETE {url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("DELETE {url} failed: {t}"),
    }
}

pub fn pub_list(api_url: &str, token: &str) -> Result<PubListResponse> {
    let url = format!("{}/pub", api_url.trim_end_matches('/'));
    match ureq::get(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(resp) => resp.into_json().context("parsing pub list response"),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            bail!("session invalid or expired. Run `spx login` to re-authenticate.")
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some(detail) = parse_error_body(&body) {
                bail!("{detail}");
            }
            bail!("GET {url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("GET {url} failed: {t}"),
    }
}

pub fn billing_checkout(api_url: &str, token: &str) -> Result<BillingCheckoutResponse> {
    let url = format!("{}/billing/checkout/", api_url.trim_end_matches('/'));
    match ureq::post(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("Content-Type", "application/json")
        .send_string("{}")
    {
        Ok(resp) => resp
            .into_json()
            .context("parsing billing checkout response"),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            bail!("session invalid or expired. Run `spx login` to re-authenticate.")
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some(detail) = parse_error_body(&body) {
                bail!("{detail}");
            }
            bail!("POST {url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("POST {url} failed: {t}"),
    }
}

pub fn billing_status(api_url: &str, token: &str) -> Result<BillingStatusResponse> {
    let url = format!("{}/billing/", api_url.trim_end_matches('/'));
    match ureq::get(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(resp) => resp.into_json().context("parsing billing status response"),
        Err(ureq::Error::Status(401, _)) | Err(ureq::Error::Status(403, _)) => {
            bail!("session invalid or expired. Run `spx login` to re-authenticate.")
        }
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some(detail) = parse_error_body(&body) {
                bail!("{detail}");
            }
            bail!("GET {url} returned {code}: {body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("GET {url} failed: {t}"),
    }
}

pub fn post_feedback<T: serde::Serialize>(
    api_url: &str,
    token: Option<&str>,
    message: &str,
    context: &T,
) -> Result<()> {
    let url = format!("{}/feedback", api_url.trim_end_matches('/'));
    let payload = FeedbackPayload { message, context };
    let body = serde_json::to_string(&payload).context("serializing feedback payload")?;
    if body.len() > MAX_FEEDBACK_BYTES {
        bail!(
            "feedback submission exceeds 1MB limit ({} bytes)",
            body.len()
        );
    }

    let mut req = ureq::post(&url).set("Content-Type", "application/json");
    if let Some(token) = token {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }

    match req.send_string(&body) {
        Ok(_) => Ok(()),
        Err(ureq::Error::Status(code, resp)) => {
            let resp_body = resp.into_string().unwrap_or_else(|_| "<no body>".into());
            if let Some(detail) = parse_error_body(&resp_body) {
                bail!("{detail}");
            }
            bail!("POST {url} returned {code}: {resp_body}");
        }
        Err(ureq::Error::Transport(t)) => bail!("POST {url} failed: {t}"),
    }
}

pub fn percent_encode_query_value(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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
}
