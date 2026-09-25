#[cfg(feature = "cache")]
pub mod cache;
#[cfg(feature = "http")]
pub mod http;
#[cfg(feature = "mail")]
pub mod mail;
#[cfg(feature = "storage")]
pub mod storage;
#[cfg(feature = "telemetry")]
pub mod telemetry;

/// Safe categories suitable for mapping to application errors; provider text is discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Configuration,
    InvalidInput,
    TooLarge,
    Unavailable,
    Rejected,
    NotFound,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Integration failure: {self:?}")
    }
}
impl std::error::Error for Error {}
