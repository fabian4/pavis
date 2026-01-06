mod common;

use anyhow::Result;
use pavis_e2e::support::PavisConfigScenario;
use reqwest::StatusCode;

#[tokio::test]
async fn test_redirect_301_response() -> Result<()> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let (_default_client, _env) = common::setup(PavisConfigScenario::RedirectDirect).await;

    let resp = client
        .get("http://localhost:8080/redirect-permanent")
        .send()
        .await?;

    assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
    let location = resp.headers().get("location").expect("location header");
    assert_eq!(location.to_str()?, "https://example.com/new-location");

    Ok(())
}

#[tokio::test]
async fn test_redirect_302_response() -> Result<()> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let (_default_client, _env) = common::setup(PavisConfigScenario::RedirectDirect).await;

    let resp = client
        .get("http://localhost:8080/redirect-temporary")
        .send()
        .await?;

    assert_eq!(resp.status(), StatusCode::FOUND);
    let location = resp.headers().get("location").expect("location header");
    assert_eq!(location.to_str()?, "https://example.com/temp");

    Ok(())
}

#[tokio::test]
async fn test_redirect_307_response() -> Result<()> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let (_default_client, _env) = common::setup(PavisConfigScenario::RedirectDirect).await;

    let resp = client
        .get("http://localhost:8080/redirect-preserve-method")
        .send()
        .await?;

    assert_eq!(resp.status(), StatusCode::TEMPORARY_REDIRECT);
    let location = resp.headers().get("location").expect("location header");
    assert_eq!(location.to_str()?, "https://example.com/v2/api");

    Ok(())
}

#[tokio::test]
async fn test_direct_200_response() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::RedirectDirect).await;

    let resp = client.get("http://localhost:8080/health").send().await?;

    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .expect("content-type header");
    assert_eq!(content_type.to_str()?, "text/plain");

    let body = resp.text().await?;
    assert_eq!(body, "OK");

    Ok(())
}

#[tokio::test]
async fn test_direct_404_response() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::RedirectDirect).await;

    let resp = client.get("http://localhost:8080/not-found").send().await?;

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let body = resp.text().await?;
    assert_eq!(body, "Resource not found");

    Ok(())
}

#[tokio::test]
async fn test_direct_503_response() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::RedirectDirect).await;

    let resp = client
        .get("http://localhost:8080/maintenance")
        .send()
        .await?;

    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = resp.text().await?;
    assert_eq!(body, "Service is under maintenance");

    Ok(())
}

#[tokio::test]
async fn test_direct_custom_json_response() -> Result<()> {
    let (client, _env) = common::setup(PavisConfigScenario::RedirectDirect).await;

    let resp = client.get("http://localhost:8080/status").send().await?;

    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.text().await?;
    assert_eq!(body, r#"{"status":"healthy","version":"1.0.0"}"#);

    Ok(())
}

#[tokio::test]
async fn test_redirect_no_follow() -> Result<()> {
    // Test that redirect responses don't follow automatically when configured
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let (_client, _env) = common::setup(PavisConfigScenario::RedirectDirect).await;

    let resp = client
        .get("http://localhost:8080/redirect-permanent")
        .send()
        .await?;

    assert_eq!(resp.status(), StatusCode::MOVED_PERMANENTLY);
    assert!(resp.headers().contains_key("location"));

    Ok(())
}
