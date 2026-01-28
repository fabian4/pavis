use anyhow::Result;
use axum_server::Handle;

use crate::common::cli::RelayArgs;
use crate::common::shutdown;
use crate::relay::routes::router;
use crate::relay::state::{AppState, RelayState};

pub async fn run(args: RelayArgs) -> Result<()> {
    let mode = args
        .mode
        .as_deref()
        .and_then(crate::relay::state::MockMode::parse);
    let state = RelayState::new_with_mode(mode);
    let app_state = AppState {
        state,
        args: args.clone(),
    };
    let app = router(app_state);

    let handle = Handle::new();
    let addr = args.listen;

    tracing::info!(%addr, "Relay listener ready");

    let server = axum_server::bind(addr)
        .handle(handle.clone())
        .serve(app.into_make_service());

    let shutdown_signal = shutdown::wait();

    tokio::select! {
        res = server => {
            res?;
        }
        _ = shutdown_signal => {
            tracing::info!("shutdown signal received");
            handle.shutdown();
        }
    }

    Ok(())
}
