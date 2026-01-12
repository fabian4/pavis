use anyhow::{Context, Result};
use clap::Parser;
use pingora::listeners::tls::TlsSettings;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use pingora::server::configuration::ServerConf;
use pingora::tls::ssl::SslVerifyMode;
use pingora::tls::x509::X509Name;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pavis::agent::{Backoff, ConfigAgent, lkg_version};
use pavis::load::{self, RuntimeLoadError};
use pavis::proxy::Proxy;
use pavis::state::RuntimeStateHandle;
use pavis::telemetry::Telemetry;
use pavis::upstream::UpstreamResolver;
use pavis_core::{AccessLogPolicy, LogLevel, WorkerCount};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    config: String,
    #[arg(long)]
    relay_url: Option<String>,
}

fn log_level_to_str(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
        #[allow(unreachable_patterns)]
        _ => "info", // Default to info for unknown log levels
    }
}

fn configure_client_auth(
    tls_settings: &mut TlsSettings,
    ca_path: &pavis_core::Path,
    require_client_cert: bool,
) -> Result<()> {
    let ca_list = X509Name::load_client_ca_file(&ca_path.0)
        .with_context(|| format!("Failed to load client CA list from {}", ca_path.0))?;
    tls_settings.set_client_ca_list(ca_list);

    tls_settings
        .set_ca_file(&ca_path.0)
        .with_context(|| format!("Failed to load client CA file {}", ca_path.0))?;

    let mut verify_mode = SslVerifyMode::PEER;
    if require_client_cert {
        verify_mode |= SslVerifyMode::FAIL_IF_NO_PEER_CERT;
    }
    tls_settings.set_verify(verify_mode);

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration (LKG)
    let config = load::load_file(&args.config).map_err(|e| match e {
        RuntimeLoadError::Pvs(pavis_pvs::PvsError::VersionMismatch { file, expected }) => {
            anyhow::anyhow!(
                "Version mismatch! File: {}, Runtime expects: {}. Recompile config.",
                file,
                expected
            )
        }
        other => anyhow::anyhow!(other),
    })?;

    let config = Arc::new(config);

    let log_level = log_level_to_str(config.telemetry.level);
    let mut filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    let p_str = log_level_to_str(config.telemetry.pingora);
    filter = filter
        .add_directive(format!("pingora={}", p_str).parse()?)
        .add_directive(format!("pingora_core={}", p_str).parse()?);

    tracing_subscriber::fmt().with_env_filter(filter).init();

    let access_log_desc = match &config.telemetry.access_log {
        AccessLogPolicy::Disabled => "off".to_string(),
        AccessLogPolicy::Stdout => "stdout".to_string(),
        AccessLogPolicy::File(path) => format!("file:{}", path.0),
        #[allow(unreachable_patterns)]
        &_ => "off".to_string(),
    };

    // Listener selection logic
    if config.listeners.is_empty() {
        anyhow::bail!("No listeners configured in runtime config.");
    }

    let max_threads = config
        .listeners
        .iter()
        .filter_map(|l| match l.workers {
            WorkerCount::Count(count) => Some(count.get() as u64),
            WorkerCount::Auto => None,
            #[allow(unreachable_patterns)]
            _ => None,
        })
        .max();

    tracing::info!(
        config = %args.config,
        listener_count = config.listeners.len(),
        max_threads = ?max_threads,
        access_log = %access_log_desc,
        "Pavis starting"
    );

    let mut server_conf = ServerConf {
        daemon: false,
        ..Default::default()
    };
    if let Some(threads) = max_threads {
        server_conf.threads = threads as usize;
    }
    let mut server = Server::new_with_opt_and_conf(None, server_conf);
    server.bootstrap();

    let runtime_state = pavis::state::RuntimeState::from_config(&config)?;
    let state_handle = Arc::new(RuntimeStateHandle::new(runtime_state));

    let lkg_version = lkg_version(Path::new(&args.config))?;

    let config_agent = args.relay_url.as_ref().map(|relay| {
        let backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(30), 200);
        let agent = ConfigAgent::new(
            relay.to_string(),
            PathBuf::from(&args.config),
            state_handle.clone(),
            Duration::from_secs(15),
            backoff,
        )?;
        agent.set_current_version(lkg_version);
        Ok::<_, anyhow::Error>(Arc::new(agent))
    });

    let (telemetry, access_log_worker) = Telemetry::new(&config.telemetry);
    let telemetry = Arc::new(telemetry);
    let resolver = UpstreamResolver::new(state_handle.clone(), Duration::from_secs(10));

    for listener in &config.listeners {
        let proxy_app = Proxy {
            state: state_handle.clone(),
            telemetry: telemetry.clone(),
        };

        let mut proxy_service = http_proxy_service(&server.configuration, proxy_app);
        let listen_addr_str = listener.address.to_string();
        match &listener.tls {
            pavis_core::TlsConfig::Disabled => {
                proxy_service.add_tcp(&listen_addr_str);
            }
            pavis_core::TlsConfig::Enabled {
                cert_path,
                key_path,
                client_auth,
            } => {
                let mut tls_settings = TlsSettings::intermediate(&cert_path.0, &key_path.0)
                    .with_context(|| {
                        format!("Failed to configure TLS for listener {}", listener.name.0)
                    })?;

                // Configure client certificate authentication
                match client_auth {
                    pavis_core::ClientAuth::Disabled => {
                        // No client certificate verification
                    }
                    pavis_core::ClientAuth::Optional { ca_path } => {
                        tracing::debug!(
                            ca_path = %ca_path.0,
                            "Configuring optional client certificate authentication"
                        );
                        configure_client_auth(&mut tls_settings, ca_path, false)?;
                    }
                    pavis_core::ClientAuth::Required { ca_path } => {
                        tracing::debug!(
                            ca_path = %ca_path.0,
                            "Configuring required client certificate authentication"
                        );
                        configure_client_auth(&mut tls_settings, ca_path, true)?;
                    }
                    #[allow(unreachable_patterns)]
                    &_ => {
                        // Unknown client auth configuration
                    }
                }

                proxy_service.add_tls_with_settings(&listen_addr_str, None, tls_settings);
            }
            #[allow(unreachable_patterns)]
            &_ => {
                proxy_service.add_tcp(&listen_addr_str);
            }
        }

        tracing::info!(
            name = %listener.name.0,
            addr = %listen_addr_str,
            "Listener registered"
        );
        server.add_service(proxy_service);
    }

    server.add_service(access_log_worker);
    server.add_service(resolver);
    if let Some(agent) = config_agent {
        let agent = agent?;
        server.add_service(agent.worker());
    }
    server.run_forever();
}

#[cfg(test)]
mod tests {
    use super::log_level_to_str;
    use pavis_core::{AccessLogPolicy, LogLevel, Path};
    use std::fs;

    #[test]
    fn log_level_to_str_defaults_to_info() {
        assert_eq!(log_level_to_str(LogLevel::Info), "info");
    }

    #[test]
    fn test_max_threads_logic() {
        use pavis_core::{Listener, ListenerName, TlsConfig, WorkerCount};
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        use std::num::NonZeroU16;

        let listener_auto = Listener {
            name: ListenerName("auto".to_string()),
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
            workers: WorkerCount::Auto,
            tls: TlsConfig::Disabled,
        };

        let listener_count = Listener {
            name: ListenerName("count".to_string()),
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8081),
            workers: WorkerCount::Count(NonZeroU16::new(4).unwrap()),
            tls: TlsConfig::Disabled,
        };

        let listeners = [listener_auto, listener_count];
        let max_threads = listeners
            .iter()
            .filter_map(|l| match l.workers {
                WorkerCount::Count(count) => Some(count.get() as u64),
                WorkerCount::Auto => None,
                #[allow(unreachable_patterns)]
                _ => None,
            })
            .max();

        assert_eq!(max_threads, Some(4));
    }

    #[test]
    fn test_log_level_to_str_all() {
        assert_eq!(log_level_to_str(LogLevel::Error), "error");
        assert_eq!(log_level_to_str(LogLevel::Warn), "warn");
        assert_eq!(log_level_to_str(LogLevel::Info), "info");
        assert_eq!(log_level_to_str(LogLevel::Debug), "debug");
        assert_eq!(log_level_to_str(LogLevel::Trace), "trace");
    }

    #[test]
    fn access_log_description_logic() {
        let desc = match AccessLogPolicy::Disabled {
            AccessLogPolicy::Disabled => "off".to_string(),
            AccessLogPolicy::Stdout => "stdout".to_string(),
            AccessLogPolicy::File(path) => format!("file:{}", path.0),
            _ => "off".to_string(),
        };
        assert_eq!(desc, "off");

        let desc = match AccessLogPolicy::Stdout {
            AccessLogPolicy::Disabled => "off".to_string(),
            AccessLogPolicy::Stdout => "stdout".to_string(),
            AccessLogPolicy::File(path) => format!("file:{}", path.0),
            _ => "off".to_string(),
        };
        assert_eq!(desc, "stdout");

        let desc = match AccessLogPolicy::File(Path("/tmp/test".to_string())) {
            AccessLogPolicy::Disabled => "off".to_string(),
            AccessLogPolicy::Stdout => "stdout".to_string(),
            AccessLogPolicy::File(path) => format!("file:{}", path.0),
            _ => "off".to_string(),
        };
        assert_eq!(desc, "file:/tmp/test");
    }

    fn write_pem(path: &std::path::Path, bytes: &[u8]) {
        fs::write(path, bytes).expect("write pem");
    }

    fn build_ca_cert() -> (
        openssl::pkey::PKey<openssl::pkey::Private>,
        openssl::x509::X509,
    ) {
        use openssl::asn1::Asn1Time;
        use openssl::hash::MessageDigest;
        use openssl::pkey::PKey;
        use openssl::rsa::Rsa;
        use openssl::x509::{X509Builder, X509NameBuilder};

        let rsa = Rsa::generate(2048).expect("generate ca key");
        let pkey = PKey::from_rsa(rsa).expect("ca pkey");

        let mut name = X509NameBuilder::new().expect("ca name");
        name.append_entry_by_text("CN", "Pavis Test CA")
            .expect("ca name cn");
        let name = name.build();

        let mut builder = X509Builder::new().expect("ca builder");
        builder.set_version(2).expect("ca version");
        builder.set_subject_name(&name).expect("ca subject");
        builder.set_issuer_name(&name).expect("ca issuer");
        builder.set_pubkey(&pkey).expect("ca pubkey");
        builder
            .set_not_before(&Asn1Time::days_from_now(0).expect("ca not_before"))
            .expect("ca not_before set");
        builder
            .set_not_after(&Asn1Time::days_from_now(365).expect("ca not_after"))
            .expect("ca not_after set");
        builder
            .sign(&pkey, MessageDigest::sha256())
            .expect("ca sign");

        (pkey, builder.build())
    }

    fn build_server_cert(
        ca_key: &openssl::pkey::PKey<openssl::pkey::Private>,
        ca_cert: &openssl::x509::X509,
    ) -> (
        openssl::pkey::PKey<openssl::pkey::Private>,
        openssl::x509::X509,
    ) {
        use openssl::asn1::Asn1Time;
        use openssl::hash::MessageDigest;
        use openssl::pkey::PKey;
        use openssl::rsa::Rsa;
        use openssl::x509::{X509Builder, X509NameBuilder};

        let rsa = Rsa::generate(2048).expect("server key");
        let pkey = PKey::from_rsa(rsa).expect("server pkey");

        let mut name = X509NameBuilder::new().expect("server name");
        name.append_entry_by_text("CN", "localhost")
            .expect("server name cn");
        let name = name.build();

        let mut builder = X509Builder::new().expect("server builder");
        builder.set_version(2).expect("server version");
        builder.set_subject_name(&name).expect("server subject");
        builder
            .set_issuer_name(ca_cert.subject_name())
            .expect("server issuer");
        builder.set_pubkey(&pkey).expect("server pubkey");
        builder
            .set_not_before(&Asn1Time::days_from_now(0).expect("server not_before"))
            .expect("server not_before set");
        builder
            .set_not_after(&Asn1Time::days_from_now(365).expect("server not_after"))
            .expect("server not_after set");
        builder
            .sign(ca_key, MessageDigest::sha256())
            .expect("server sign");

        (pkey, builder.build())
    }

    #[test]
    fn configure_client_auth_accepts_valid_ca() {
        use super::configure_client_auth;
        use pingora::listeners::tls::TlsSettings;

        let mut dir = std::env::temp_dir();
        dir.push(format!("pavis_test_ca_{}", rand::random::<u64>()));
        fs::create_dir_all(&dir).expect("create temp dir");

        let ca_path = dir.join("ca.pem");
        let cert_path = dir.join("server.pem");
        let key_path = dir.join("server.key");

        let (ca_key, ca_cert) = build_ca_cert();
        let (server_key, server_cert) = build_server_cert(&ca_key, &ca_cert);

        write_pem(&ca_path, &ca_cert.to_pem().expect("ca pem"));
        write_pem(&cert_path, &server_cert.to_pem().expect("server cert pem"));
        write_pem(
            &key_path,
            &server_key
                .private_key_to_pem_pkcs8()
                .expect("server key pem"),
        );

        let mut tls_settings = TlsSettings::intermediate(
            cert_path.to_str().expect("cert path"),
            key_path.to_str().expect("key path"),
        )
        .expect("tls settings");

        let ca_path = Path(ca_path.to_string_lossy().into_owned());
        configure_client_auth(&mut tls_settings, &ca_path, true).expect("client auth");

        fs::remove_dir_all(&dir).ok();
    }
}
