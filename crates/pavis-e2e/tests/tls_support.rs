use reqwest::Client;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// Self-signed cert for localhost generated for testing
const CERT_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIDCTCCAfGgAwIBAgIUBujbuFPhDyE0AlwoP+EEtLqL8eUwDQYJKoZIhvcNAQEL
BQAwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI1MTIyNjE2NTgwNFoXDTI2MTIy
NjE2NTgwNFowFDESMBAGA1UEAwwJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEA1cvUamqHkM4QSQBo9MfBQWEZatE6srs43sAWr+fcy4uL
C4dHN2T/eWuHosLXJ5VzIhCsLrzD0TVTg8l/fOa2OluhEXrN0xsAL3LAG7KMDvMm
vcfX87+GPFTu1fbwMVL8l41jTBikgn0oQBakI7Eheh9WtW7ZBqQlBkS3pIm6jUpE
WluUDN8nFTv3VOSCILNwoZC0evPIc5q0YerrlwE2LabTr1HZ/sIs58HfVmoXvuuU
xA2aDPTR9jteys+D10p04QMgh2iKmCQ7SuiQjqNlQXaTHW3vk4MotvrN9lO3a30H
Zx31FN5unyFjML+o6xTa+VA7Xa4WHbs0tKPvZ4HjLQIDAQABo1MwUTAdBgNVHQ4E
FgQUrNw8AllfOdsxEFemjz0rvb22GCMwHwYDVR0jBBgwFoAUrNw8AllfOdsxEFem
pj0rvb22GCMwDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAOQD8
T2VxIt3Y7EtUEfI6LYVU3anwtxsCK7+ckd2cVQRJKhdzAiyBAB/xgGxG/Q2At2Zm
E0KtvnkDuNIeYJKiy+BcfE0UdORQOvj8Xznp/4/V2KH6RoN671Jo3APulP/T3ZN8
ub+CTdbPNimisDRJxJP68/i6j3n+UQ8aCMNbs+0bbP1v5p0zkw5T5U/4UKctQrIj
Sw1ILyJ9w4FEHYAsoL4CWvKTurprB17ZJ/W76GhxCBKhDY0IIOgrX05QpO3Odfss
exekMA4TqyWPNGpgQd0tKdN3toFbk1Omy6AM0nW9Fl7eYdWC1arOQtaj6OvDn4bW
AM6P6fbKhtdya5+Avg==
-----END CERTIFICATE-----"#;

const KEY_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDVy9RqaoeQzhBJ
AGj0x8FBYRlq0TqyuzjewBav59zLi4sLh0c3ZP95a4eiwtcnlXMiEKwuvMPRNVOD
yX985rY6W6ERes3TGwAvcsAbsowO8ya9x9fzv4Y8VO7V9vAxUvyXjWNMGKSCfShA
FqQjsSF6H1a1btkGpCUGRLekibqNSkRaW5QM3ycVO/dU5IIgs3ChkLR688hzmrRh
6uuXATYtptOvUdn+wiznwd9Wahe+65TEDZoM9NH2O17Kz4PXSnThAyCHaIqYJDtK
6JCOo2VBdpMdbe+Tgyi2+s32U7drfQdnHfUU3m6fIWMwv6jrFNr5UDtdrhYduzS0
o+9ngeMtAgMBAAECggEAOvRyJsYpi+zG4NqUFqvexsCX2bTIkvC9xe8CUo+FezuH
pC4xnTzklf7o8CD6Y5f6n7IpSNoUxWQHG5g855xXM9CAoelTSJtxeaQTBZA+vwCd
9kddbYGq6oghNC9cHL3dmY0LjLhe5PzOVJ3ptU3rBVoO9wkSH5qz+v6IBX9VShBn
skLUns1lrY8ox+CfMdmLyPQgCuMMnjUuUjoRkp1F1kKgcRkRcJ4Kd737yCu5ONTW
PRbOkSMGaEio8IfW/5J3ghxsSU7qG2eA5GFkquS3CFQMgJwsAI9xFJSwFUHJpar6
0Ke2XgjoRHGyi2k2UK7vplFI8FSGtJw97ERoDFgVoQKBgQDqU7WlK6x5PCuM4j7x
QrsXrGQG3iiVfehf7Zd3sF9Wk89KtszAPzhppW8rTL/ugYZ1BVfH/3ScbYeDgb1a
jkN4a7Dv7UJ+CixD3HIGA1xENcT5nVZpIyF/8QOkzImXZjVDt4llHB30j/PIDhXK
Ij3N9khNQeMNq1c2l4qH30lExwKBgQDpkgBhxH+QHXGO87QVpfoxtkfcp/gVIzpB
T5p0D6wQ9drW/6NSf/hv4jeun9OePU8m+G8q5mGeMu3cg+AUGhcXXVqZO5c2OaHk
c4fAbQApIjOgr+fADdqNobu2c4mLr8SkIVKCEI9ubKScbEVwuDhkT4MC0vYcJjHe
CR8Cb6C8awKBgA/27gw3woNr/weVLnafdkGxpAr3vcoZjuhiNoyX/pbWcSwE8kQy
ynQgKkfH7dehCXkViRp+JAK4T6A9CZqO0Lf2llJyVrJhnQxui3IvbmzTQP1Eo+t7
0j92OypSKRmghAZ+DaVO2hecax55HzDrTkym99wTnhWDU+jLQEvrgYFnAoGBAK1z
Os1fussu0lGyMJ2S8EVSc/Ms2VH5Ix21G6HssX621JispoBxf/C2MVuAXQo5xTnP
a96TzxJIB9OmKxVCertjHBCG7DfcfJjGIp2HVIM3XteJSbSZlR9wZ5GKIy6UjJbG
GBt2aM076NIwpTCb3WTAly3Vs+YbhxS3+Us50keZAoGBAJXf1NKZG70K1CpyCeHC
y4mhnWmKvxaECmYy60wm4SVtyWmCCni3xUKZqHvx9yXrBf3H1VgRNhaUr9674Z9v
hatNEGh7mtSoh5jEPWhEIrkCcezKCTZMhgk3zgqWreV5iV/m5jNs1IV2YBXXXueT
A1DP0vFyr8ikIQzD+viwK6LX
-----END PRIVATE KEY-----"#;

async fn start_backend() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind backend");
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        loop {
            if let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0; 1024];
                    // Read request (simple ignore)
                    if socket.read(&mut buf).await.is_ok() {
                        // Write response
                        let response = "HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nHello Backend";
                        let _ = socket.write_all(response.as_bytes()).await;
                    }
                });
            }
        }
    });

    port
}

fn find_binary() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap();
    loop {
        let candidate = dir.join("target/debug/pavis");
        if candidate.exists() {
            return candidate;
        }
        let candidate = dir.join("target/release/pavis");
        if candidate.exists() {
            return candidate;
        }

        if !dir.pop() {
            panic!("Could not find pavis binary at {}", dir.display());
        }
    }
}

#[tokio::test]
async fn test_tls_support() {
    // 1. Setup Backend
    let backend_port = start_backend().await;

    // 2. Setup Certs
    let tmp_dir = std::env::temp_dir().join("pavis_test_tls");
    let _ = fs::remove_dir_all(&tmp_dir);
    fs::create_dir_all(&tmp_dir).unwrap();

    let cert_path = tmp_dir.join("cert.pem");
    let key_path = tmp_dir.join("key.pem");
    let config_path = tmp_dir.join("config.yaml");

    fs::write(&cert_path, CERT_PEM).unwrap();
    fs::write(&key_path, KEY_PEM).unwrap();

    // 3. Config
    let config = format!(
        r#"server:
  listen_addr: "0.0.0.0:8443"
  tls:
    enabled: true
    cert_path: "{}"
    key_path: "{}"
telemetry:
  level: "debug"
  access_log: "stdout"
upstreams:
  - name: "backend"
    endpoints:
      - ip: "127.0.0.1"
        port: {}
routes:
  - host: "*"
    paths:
      - path: "/"
        destinations:
          - upstream: "backend"
            weight: 1
"#,
        cert_path.display(),
        key_path.display(),
        backend_port
    );

    fs::write(&config_path, config).unwrap();

    // 4. Start Pavis
    let binary = find_binary();
    println!("Starting binary: {:?}", binary);
    let mut child = Command::new(binary)
        .arg("--config")
        .arg(&config_path)
        .stdout(Stdio::piped()) // Capture to avoid noise, change to inherit if debugging needed
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to start pavis");

    // Wait for start
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 5. Make Request
    let client = Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();

    let resp = client.get("https://localhost:8443/").send().await;

    // 6. Cleanup
    let _ = child.kill();
    let _ = fs::remove_dir_all(&tmp_dir);

    // Assert
    match resp {
        Ok(r) => {
            assert!(
                r.status().is_success(),
                "Response was not success: {:?}",
                r.status()
            );
            let text = r.text().await.unwrap();
            assert_eq!(text, "Hello Backend");
        }
        Err(e) => {
            panic!("Request failed: {}", e);
        }
    }
}
