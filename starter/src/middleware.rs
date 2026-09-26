//! Remove entries to disable common middleware. Bracel preserves execution order.
use bracel::http::middleware::Middleware;

pub const ENABLED: &[Middleware] = &[
    Middleware::RequestContext,
    Middleware::Cors,
    Middleware::Timeout,
    Middleware::BodyLimit,
    Middleware::RateLimit,
    Middleware::ConcurrencyLimit,
    Middleware::Compression,
];
