//! One bounded process-local cache shared by AppState clones.
pub use bracel_integrations::cache::ScopedCache;
use std::time::Duration;

#[derive(Clone)]
pub struct Settings {
    capacity_bytes: u64,
    ttl: Duration,
    max_item_bytes: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capacity_bytes: 16 * 1024 * 1024,
            ttl: Duration::from_secs(60),
            max_item_bytes: 1024 * 1024,
        }
    }
}

impl Settings {
    pub fn from_lookup(get: &impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let number = |key: &str, default: u64, max: u64| -> Result<u64, String> {
            let value = get(key).map_or(Ok(default), |s| {
                s.parse::<u64>()
                    .map_err(|_| format!("{key} must be an integer"))
            })?;
            if value == 0 || value > max {
                return Err(format!("{key} must be between 1 and {max}"));
            }
            Ok(value)
        };
        let settings = Self {
            capacity_bytes: number("CACHE_CAPACITY_BYTES", 16 * 1024 * 1024, 1024 * 1024 * 1024)?,
            ttl: Duration::from_secs(number("CACHE_TTL_SECONDS", 60, 86400)?),
            max_item_bytes: number("CACHE_MAX_ITEM_BYTES", 1024 * 1024, 16 * 1024 * 1024)? as usize,
        };
        if settings.max_item_bytes as u64 > settings.capacity_bytes {
            return Err("CACHE_MAX_ITEM_BYTES cannot exceed CACHE_CAPACITY_BYTES".into());
        }
        Ok(settings)
    }

    pub fn build(&self) -> ScopedCache {
        ScopedCache::new(self.capacity_bytes, self.ttl, self.max_item_bytes)
            .expect("validated cache settings")
    }
}
