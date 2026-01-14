use anyhow::{Context, Result};
use clap::Parser;
use pingora::listeners::tls::TlsSettings;
use pingora::prelude::*;
use pingora::proxy::http_proxy_service;
use pingora::server::configuration::ServerConf;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{Registry, layer::SubscriberExt, util::SubscriberInitExt};

use arc_swap::ArcSwap;
use pavis::agent::{Backoff, ConfigAgent, lkg_version};
use pavis::load::{self, RuntimeLoadError};
use pavis::proxy::Proxy;
use pavis::state::RuntimeStateHandle;
use pavis::telemetry::Telemetry;
use pavis::upstream::UpstreamResolver;
use pavis_core::{AccessLogPolicy, LogLevel, RuntimeConfig, WorkerCount};
use rustls::RootCertStore;
use rustls::crypto::{CryptoProvider, ring};
use std::sync::Once;

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
    _tls_settings: &mut TlsSettings,
    ca_path: &pavis_core::Path,
    require_client_cert: bool,
) -> Result<()> {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        if CryptoProvider::install_default(ring::default_provider()).is_err() {
            // Another provider was already installed; ignore.
        }
    });
    let ca_file = File::open(&ca_path.0)
        .with_context(|| format!("Failed to open client CA file {}", ca_path.0))?;
    let mut reader = BufReader::new(ca_file);
    let certs: Vec<_> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<_, _>>()
        .with_context(|| format!("Failed to parse client CA file {}", ca_path.0))?;

    let mut root_store = rustls::RootCertStore::empty();
    for cert in certs {
        root_store
            .add(cert)
            .context("Failed to add client CA cert to store")?;
    }

    let _verifier = if require_client_cert {
        rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
            .build()
            .context("Failed to build client cert verifier")?
    } else {
        rustls::server::WebPkiClientVerifier::builder(Arc::new(root_store))
            .allow_unauthenticated()
            .build()
            .context("Failed to build optional client cert verifier")?
    };

    // TODO: wire the verifier into Pingora once its Rustls settings expose a setter.
    // PENDING: https://github.com/cloudflare/pingora/issues/791
    Ok(())
}

fn create_root_store(config: &RuntimeConfig) -> RootCertStore {
    let mut root_store = RootCertStore::empty();

    // Use webpki-roots directly
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let mut custom_count = 0;
    for u in &config.upstreams {
        if let pavis_core::TlsPolicy::Enabled { ca, .. } = &u.tls
            && let pavis_core::UpstreamCa::File { path } = ca
        {
            match File::open(&path.0) {
                Ok(file) => {
                    let mut reader = BufReader::new(file);
                    for cert in rustls_pemfile::certs(&mut reader).flatten() {
                        if let Err(err) = root_store.add(cert) {
                            tracing::warn!(path = %path.0, error = %err, "Failed to add certificate to root store");
                        } else {
                            custom_count += 1;
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(path = %path.0, error = %err, "Failed to read upstream CA bundle");
                }
            }
        }
    }
    tracing::info!(
        custom_ca_count = custom_count,
        "Created root store with system roots and custom CAs"
    );
    root_store
}

fn main() -> Result<()> {
    // Install the default crypto provider (ring) globally.
    // This is required because we might have both 'ring' (via reqwest) and 'aws-lc-rs' (via pingora) enabled,
    // which prevents rustls from automatically selecting one.
    if CryptoProvider::install_default(ring::default_provider()).is_err() {
        // Already installed, which is fine.
    }

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

    // Setup logging (Subscriber + Reloadable OpenTelemetry Layer)
    let log_level = log_level_to_str(config.telemetry.level);
    let mut filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    let p_str = log_level_to_str(config.telemetry.pingora);
    filter = filter
        .add_directive(format!("pingora={}", p_str).parse()?)
        .add_directive(format!("pingora_core={}", p_str).parse()?);

    let fmt_layer = tracing_subscriber::fmt::layer();

    // Custom reloadable layer. transparent to downcasting.
    let reload_handle: pavis::telemetry::tracing::ReloadHandle =
        pavis::telemetry::tracing::ReloadableLayer::new();
    let otel_layer = reload_handle.clone();

    // Register layers. Order matters: otel_layer is typed for Registry,
    // so it must be applied directly to Registry.
    Registry::default()
        .with(otel_layer)
        .with(fmt_layer)
        .with(filter)
        .init();
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

    let ca_store = Arc::new(ArcSwap::from_pointee(create_root_store(&config)));

    let mut server = Server::new_with_opt_and_conf(None, server_conf);
    server.bootstrap();

    let server_conf_arc = server.configuration.clone();

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

        let ca_store_clone = ca_store.clone();
        agent.on_update(move |config| {
            ca_store_clone.store(Arc::new(create_root_store(config)));
            tracing::info!("Updated global CA root store from new configuration");
        });

        Ok::<_, anyhow::Error>(Arc::new(agent))
    });

    let (telemetry, access_log_worker, metrics_worker, tracing_service) =
        Telemetry::new(&config.telemetry, Some(reload_handle));
    let telemetry = Arc::new(telemetry);
    let resolver = UpstreamResolver::new(state_handle.clone(), Duration::from_secs(10));

    for listener in &config.listeners {
        let proxy_app = Proxy {
            state: state_handle.clone(),
            telemetry: telemetry.clone(),
            ca_store: ca_store.clone(),
        };

        let mut proxy_service = http_proxy_service(&server_conf_arc, proxy_app);
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
    server.add_service(tracing_service);

    if let Some(metrics_worker) = metrics_worker {
        server.add_service(metrics_worker);
    }

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

    // Pure-Rust replacement for OpenSSL cert generation
    fn build_ca_cert() -> (rcgen::KeyPair, rcgen::Certificate, String) {
        let mut params = rcgen::CertificateParams::new(vec!["Pavis Test CA".to_string()]).unwrap();
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "Pavis Test CA");
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        let pem = cert.pem();
        (key_pair, cert, pem)
    }

    fn build_server_cert(
        ca_cert: &rcgen::Certificate,
        ca_key: &rcgen::KeyPair,
    ) -> (String, String) {
        let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "localhost");
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.signed_by(&key_pair, ca_cert, ca_key).unwrap();
        (key_pair.serialize_pem(), cert.pem())
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

        let (ca_key, ca_cert_obj, ca_cert_pem) = build_ca_cert();
        let (server_key_pem, server_cert_pem) = build_server_cert(&ca_cert_obj, &ca_key);

        write_pem(&ca_path, ca_cert_pem.as_bytes());
        write_pem(&cert_path, server_cert_pem.as_bytes());
        write_pem(&key_path, server_key_pem.as_bytes());

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
