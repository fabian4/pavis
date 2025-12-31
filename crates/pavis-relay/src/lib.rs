pub mod config;

mod app;
mod codec;
mod handlers;
mod ingest;
mod pipeline;
mod routes;
mod state;

pub use app::serve_from_config;

#[cfg(test)]
mod http_tests;
