use anyhow::Result;
use bytes::Bytes;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use pavis_ingest_api::{Artifact, IngestError, SourceInfo};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::{infer_format, validate_bytes, validate_format};

pub async fn spawn_watcher(
    path: PathBuf,
    debounce: Duration,
    tx: mpsc::Sender<Result<Artifact, IngestError>>,
) -> Result<RecommendedWatcher, IngestError> {
    let (event_tx, mut event_rx) = mpsc::channel(100);

    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = event_tx.blocking_send(event);
            }
        },
        Config::default(),
    )
    .map_err(|e| IngestError::Io(anyhow::anyhow!(e)))?;

    watcher
        .watch(&path, RecursiveMode::NonRecursive)
        .map_err(|e| IngestError::Io(anyhow::anyhow!(e)))?;

    let ingest_path = path.clone();

    tokio::spawn(async move {
        let mut debounce_timer: Option<Pin<Box<tokio::time::Sleep>>> = None;

        let mut last_mtime = tokio::fs::metadata(&ingest_path)
            .await
            .and_then(|m| m.modified())
            .ok();

        debug!(
            "Watcher starting for: {:?}, initial mtime: {:?}",
            ingest_path, last_mtime
        );

        let mut poll_interval = tokio::time::interval(Duration::from_secs(2));

        loop {
            let timer_fired = async {
                if let Some(timer) = debounce_timer.as_mut() {
                    timer.await;

                    true
                } else {
                    futures_util::future::pending().await
                }
            };

            tokio::select! {
                _ = poll_interval.tick() => {
                    let mtime = tokio::fs::metadata(&ingest_path)
                        .await
                        .and_then(|m| m.modified())
                        .ok();
                    if mtime != last_mtime {
                        debug!("File change detected via polling: {:?}, old_mtime={:?}, new_mtime={:?}", ingest_path, last_mtime, mtime);
                        last_mtime = mtime;
                        debounce_timer = Some(Box::pin(tokio::time::sleep(debounce)));
                    }
                }
                Some(event) = event_rx.recv() => {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any => {
                            let mtime = tokio::fs::metadata(&ingest_path)
                                .await
                                .and_then(|m| m.modified())
                                .ok();
                            debug!("File event detected: {:?}, mtime={:?}", event.kind, mtime);
                            last_mtime = mtime;
                            debounce_timer = Some(Box::pin(tokio::time::sleep(debounce)));
                        }
                        _ => {}
                    }
                }
                _ = timer_fired => {
                    debounce_timer = None;
                    debug!("Debounce expired, reading file: {:?}", ingest_path);

                    let format = infer_format(&ingest_path);
                    if let Err(err) = validate_format(&ingest_path, format) {
                        warn!("Rejected unsupported file format: {:?}", ingest_path);
                        if let Err(send_err) = tx.send(Err(err)).await {
                            error!("Failed to send error through stream: {}", send_err);
                            break;
                        }
                        continue;
                    }

                    match tokio::fs::read(&ingest_path).await {
                        Ok(bytes) => {
                            if let Err(err) = validate_bytes(&ingest_path, &bytes) {
                                warn!("Rejected file payload: {:?}", ingest_path);
                                if let Err(send_err) = tx.send(Err(err)).await {
                                    error!("Failed to send error through stream: {}", send_err);
                                    break;
                                }
                                continue;
                            }
                            debug!("Read {} bytes from: {:?}", bytes.len(), ingest_path);
                            let source = SourceInfo::new(ingest_path.to_string_lossy());
                            let art = Artifact::new(Bytes::from(bytes), format, source);
                            if let Err(e) = tx.send(Ok(art)).await {
                                error!("Failed to send artifact through stream: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Failed to read file after event: {}", e);
                            if let Err(send_err) = tx.send(Err(IngestError::Io(anyhow::anyhow!(e)))).await {
                                error!("Failed to send error through stream: {}", send_err);
                                break;
                            }
                        }
                    }
                }
            }
        }
    });

    Ok(watcher)
}
