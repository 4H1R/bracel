//! Bracel: explicit Axum applications with shared HTTP contracts and policies.
//! Applications own their state, routes, readiness checks and migrations.
pub mod authorization;
pub mod commands;
pub mod config;
pub mod http;
pub mod identity;
#[cfg(feature = "jobs")]
pub mod jobs;
pub mod query;
#[cfg(feature = "testing")]
pub mod testing;
#[cfg(feature = "tokens")]
pub mod tokens;

pub use axum;
pub use http::Application;
pub use sea_orm;
pub use utoipa;
pub use utoipa_axum;
