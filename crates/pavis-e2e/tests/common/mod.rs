use pavis_e2e::support::TestEnv;
use reqwest::Client;
use std::time::Duration;

pub async fn setup(config_name: &str) -> (Client, TestEnv) {
    let env = TestEnv::new(config_name)
        .await
        .expect("Failed to setup test env");

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to build reqwest client");

    (client, env)
}
