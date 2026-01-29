use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct PublishResponse {
    version: u64,
    checksum: String,
    size: u64,
    published_at: String,
}

pub(crate) fn publish_to_relay(relay_base: &str, artifact: &Path) -> Result<()> {
    let bytes = std::fs::read(artifact)
        .with_context(|| format!("failed to read artifact {}", artifact.display()))?;
    let url = format!("{}/v1/publish", relay_base.trim_end_matches('/'));

    let mut response = ureq::post(&url)
        .config()
        .http_status_as_error(false)
        .build()
        .send(bytes.as_slice())
        .context("failed to send publish request")?;

    let status = response.status();
    let body = response
        .body_mut()
        .read_to_string()
        .context("failed to read publish response body")?;

    if !status.is_success() {
        anyhow::bail!(
            "publish failed: status={}, body={}",
            status.as_u16(),
            body.trim()
        );
    }

    let parsed: PublishResponse =
        serde_json::from_str(&body).context("failed to parse publish response")?;

    println!("Published config to relay");
    println!("  Version:      {}", parsed.version);
    println!("  Checksum:     {}", parsed.checksum);
    println!("  Size:         {} bytes", parsed.size);
    println!("  Published At: {}", parsed.published_at);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn try_mock_server() -> Option<mockito::ServerGuard> {
        std::panic::catch_unwind(|| mockito::Server::new())
            .map_err(|_| {
                eprintln!("mockito server unavailable; skipping test");
            })
            .ok()
    }

    #[test]
    fn test_publish_success() {
        let mut server = match try_mock_server() {
            Some(server) => server,
            None => return,
        };
        let mock = server
            .mock("POST", "/v1/publish")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"version": 1, "checksum": "abc", "size": 100, "published_at": "now"}"#)
            .create();

        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"test content").unwrap();

        let result = publish_to_relay(&server.url(), file.path());
        assert!(result.is_ok());
        mock.assert();
    }

    #[test]
    fn test_publish_server_error() {
        let mut server = match try_mock_server() {
            Some(server) => server,
            None => return,
        };
        let mock = server
            .mock("POST", "/v1/publish")
            .with_status(500)
            .with_body("internal server error")
            .create();

        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"test content").unwrap();

        let result = publish_to_relay(&server.url(), file.path());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("publish failed: status=500")
        );
        mock.assert();
    }
}
