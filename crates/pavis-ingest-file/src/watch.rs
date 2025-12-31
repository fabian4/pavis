use anyhow::Result;
use bytes::Bytes;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use pavis_ingest_api::{Artifact, IngestError, SourceInfo};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use crate::{infer_format, is_supported};

pub async fn spawn_watcher(
    path: PathBuf,
    debounce: Duration,
    tx: mpsc::Sender<Result<Artifact, IngestError>>,
) -> Result<RecommendedWatcher, IngestError> {
    let (event_tx, mut event_rx) = mpsc::channel(10);

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

        loop {
            tokio::select! {
                Some(event) = event_rx.recv() => {
                    match event.kind {
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any => {
                            debug!("File event detected: {:?}", event.kind);
                            debounce_timer = Some(Box::pin(tokio::time::sleep(debounce)));
                        }
                        _ => {}
                    }
                }
                Some(_) = async {
                    if let Some(timer) = debounce_timer.as_mut() {
                        timer.await;
                        Some(())
                    } else {
                        None
                    }
                }, if debounce_timer.is_some() => {
                    debounce_timer = None;
                    debug!("Debounce expired, reading file");

                    let format = infer_format(&ingest_path);
                    if !is_supported(format) {
                        warn!("Ignored unsupported file format: {:?}", ingest_path);
                        continue;
                    }

                    match tokio::fs::read(&ingest_path).await {
                        Ok(bytes) => {
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
                else => break,
            }
        }
    });

    Ok(watcher)
}
