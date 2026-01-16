pub mod config;

mod app;
mod codec;
mod handlers;
mod ingest;
mod pipeline;
mod routes;
mod runtime;
mod state;
mod storage;

pub use app::serve_from_config;

#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod integration;
