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

const WATCHER_POLL_INTERVAL: Duration = Duration::from_secs(2);

pub async fn spawn_watcher(
    path: PathBuf,
    debounce: Duration,
    max_bytes: u64,
    tx: mpsc::Sender<Result<Artifact, IngestError>>,
) -> Result<RecommendedWatcher, IngestError> {
    let (event_tx, mut event_rx) = mpsc::channel(100);

    let config = Config::default().with_poll_interval(WATCHER_POLL_INTERVAL);
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = event_tx.blocking_send(event);
            }
        },
        config,
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

        let mut poll_interval = tokio::time::interval(WATCHER_POLL_INTERVAL);

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
                biased;
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

                    let size = tokio::fs::metadata(&ingest_path)
                        .await
                        .map(|meta| meta.len())
                        .map_err(|e| IngestError::Io(anyhow::anyhow!(e)));

                    if let Ok(size) = size
                        && let Err(err) = crate::validate_size(&ingest_path, size, max_bytes)
                    {
                        warn!("Rejected file size: {:?}", ingest_path);
                        if let Err(send_err) = tx.send(Err(err)).await {
                            error!("Failed to send error through stream: {}", send_err);
                            break;
                        }
                        continue;
                    }

                    match tokio::fs::read(&ingest_path).await {
                        Ok(bytes) => {
                            if let Err(err) =
                                crate::validate_size(&ingest_path, bytes.len() as u64, max_bytes)
                            {
                                warn!("Rejected file size: {:?}", ingest_path);
                                if let Err(send_err) = tx.send(Err(err)).await {
                                    error!("Failed to send error through stream: {}", send_err);
                                    break;
                                }
                                continue;
                            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_spawn_watcher_invalid_format() {
        let tmp = NamedTempFile::new().unwrap();
        // Create an invalid extension file
        let path = tmp.path().with_extension("txt");
        std::fs::rename(tmp.path(), &path).unwrap();

        let (tx, mut rx) = mpsc::channel(1);
        let _watcher = spawn_watcher(path.clone(), Duration::from_millis(10), 1024, tx)
            .await
            .unwrap();

        // Write to trigger event
        std::fs::write(&path, "data").unwrap();

        // Should get an error about unsupported format
        let res = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(res.is_err());
        assert!(
            res.unwrap_err()
                .to_string()
                .contains("Unsupported file format")
        );
    }

    #[tokio::test]
    async fn test_spawn_watcher_oversized_file() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("yaml");
        std::fs::rename(tmp.path(), &path).unwrap();

        let (tx, mut rx) = mpsc::channel(1);
        let _watcher = spawn_watcher(path.clone(), Duration::from_millis(10), 1, tx)
            .await
            .unwrap(); // max 1 byte

        // Write 10 bytes
        std::fs::write(&path, "large data").unwrap();

        let res = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("exceeds max_bytes"));
    }

    #[tokio::test]
    async fn test_spawn_watcher_read_error() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("yaml");
        std::fs::rename(tmp.path(), &path).unwrap();

        let (tx, mut rx) = mpsc::channel(1);
        let _watcher = spawn_watcher(path.clone(), Duration::from_millis(10), 1024, tx)
            .await
            .unwrap();

        // Delete file to cause read error
        std::fs::remove_file(&path).unwrap();

        // Trigger by waiting for poll interval if events don't work on delete
        // Actually, deleting the file should trigger an event on some OSes,
        // or the poll loop will see mtime mismatch (None vs initial)

        let res = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_spawn_watcher_mpsc_send_failure() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("yaml");
        std::fs::rename(tmp.path(), &path).unwrap();

        let (tx, rx) = mpsc::channel(1);
        let _watcher = spawn_watcher(path.clone(), Duration::from_millis(10), 1024, tx)
            .await
            .unwrap();

        // Drop receiver to trigger send failure later
        drop(rx);

        // Trigger event
        std::fs::write(&path, "data").unwrap();

        // Wait for debounce and loop to exit
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
