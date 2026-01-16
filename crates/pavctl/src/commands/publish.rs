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

    let response = match ureq::post(&url).send_bytes(&bytes) {
        Ok(response) => response,
        Err(ureq::Error::Status(status, response)) => {
            let body = response.into_string().unwrap_or_default();
            anyhow::bail!("publish failed: status={status}, body={body}");
        }
        Err(err) => return Err(err.into()),
    };

    let body = response
        .into_string()
        .context("failed to read publish response body")?;
    let parsed: PublishResponse =
        serde_json::from_str(&body).context("failed to parse publish response")?;

    println!("Published config to relay");
    println!("  Version:      {}", parsed.version);
    println!("  Checksum:     {}", parsed.checksum);
    println!("  Size:         {} bytes", parsed.size);
    println!("  Published At: {}", parsed.published_at);

    Ok(())
}
