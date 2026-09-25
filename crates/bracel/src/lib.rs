//! Bracel: explicit Axum applications with shared HTTP contracts and policies.
//! Applications own their state, routes, readiness checks and migrations.
pub mod config;
pub mod http;
pub mod identity;
pub mod query;

pub use axum;
pub use http::Application;
pub use sea_orm;
pub use utoipa;
