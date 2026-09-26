/// Optional middleware installed in a fixed order. Authentication remains an
/// explicit route policy and cannot be disabled by removing another layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Middleware {
    /// Request IDs, safe logs, metrics, error normalization and nosniff headers.
    RequestContext,
    Cors,
    Timeout,
    BodyLimit,
    RateLimit,
    ConcurrencyLimit,
    /// Also requires the compression Cargo feature and HTTP_COMPRESSION=true.
    Compression,
}

impl Middleware {
    pub const DEFAULTS: &'static [Self] = &[
        Self::RequestContext,
        Self::Cors,
        Self::Timeout,
        Self::BodyLimit,
        Self::RateLimit,
        Self::ConcurrencyLimit,
        Self::Compression,
    ];
}
