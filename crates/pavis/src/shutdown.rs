//! Shutdown coordinator for graceful shutdown.

use pavis_core::ShutdownPolicy;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::watch;
use tokio::time::sleep;

/// Coordinates graceful shutdown across the runtime.
///
/// The coordinator waits for SIGTERM or SIGINT and broadcasts shutdown via a watch channel.
/// All background services should subscribe to the shutdown channel and clean up when signaled.
pub struct ShutdownCoordinator {
    policy: ShutdownPolicy,
    shutdown_tx: watch::Sender<bool>,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator.
    ///
    /// Returns the coordinator and a receiver that background services can use to
    /// subscribe to shutdown notifications.
    pub fn new(policy: ShutdownPolicy) -> (Self, watch::Receiver<bool>) {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        (
            Self {
                policy,
                shutdown_tx,
            },
            shutdown_rx,
        )
    }

    /// Wait for a shutdown signal (SIGTERM or SIGINT).
    ///
    /// If shutdown is Enabled, waits for the drain_timeout duration after
    /// receiving the signal before returning.
    ///
    /// # Errors
    /// Returns an error if signal handlers cannot be installed.
    pub async fn wait_for_signal(&self) -> std::io::Result<()> {
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;

        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, initiating shutdown");
            }
            _ = sigint.recv() => {
                tracing::info!("Received SIGINT, initiating shutdown");
            }
        }

        // Broadcast shutdown to all subscribers
        if self.shutdown_tx.send(true).is_err() {
            tracing::warn!("No shutdown subscribers, skipping broadcast");
        }

        // If graceful shutdown is enabled, wait for drain timeout
        match self.policy {
            ShutdownPolicy::Disabled => {
                tracing::info!("Graceful shutdown disabled, exiting immediately");
            }
            ShutdownPolicy::Enabled { drain_timeout } => {
                let timeout_ms = drain_timeout.0.get();
                tracing::info!(
                    timeout_ms,
                    "Graceful shutdown enabled, draining for {} ms",
                    timeout_ms
                );
                sleep(std::time::Duration::from_millis(timeout_ms as u64)).await;
                tracing::info!("Drain timeout elapsed, forcing shutdown");
            }
            #[allow(unreachable_patterns)]
            _ => {
                tracing::warn!("Unknown shutdown policy, exiting immediately");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pavis_core::Duration;
    use std::num::NonZeroU32;

    #[tokio::test]
    async fn shutdown_coordinator_broadcasts() {
        let policy = ShutdownPolicy::Disabled;
        let (_coordinator, rx) = ShutdownCoordinator::new(policy);

        // Initially not shutdown
        assert!(!*rx.borrow());

        // Simulate shutdown broadcast
        // (we can't test signal handling in unit tests, so we just verify the channel works)
    }

    #[test]
    fn shutdown_coordinator_creates_channel() {
        let policy = ShutdownPolicy::Enabled {
            drain_timeout: Duration(NonZeroU32::new(1000).unwrap()),
        };
        let (_coordinator, rx) = ShutdownCoordinator::new(policy);
        assert!(!*rx.borrow(), "should start not-shutdown");
    }
}
