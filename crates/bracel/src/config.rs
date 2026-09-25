use std::{net::SocketAddr, time::Duration};

/// Validated HTTP settings. Applications own database and domain configuration.
#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub request_timeout: Duration,
    pub body_limit: usize,
    pub auth: Option<crate::identity::BearerAuth>,
    pub cors_origins: Vec<axum::http::HeaderValue>,
    pub anonymous_per_minute: u32,
    pub authenticated_per_minute: u32,
    pub writes_per_minute: u32,
    pub rate_max_keys: usize,
    pub max_in_flight: usize,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let bind = get("BIND_ADDR")
            .unwrap_or_else(|| "127.0.0.1:3000".into())
            .parse()
            .map_err(|_| "BIND_ADDR must be an IP socket address")?;
        let number = |name, default, max| -> Result<u64, String> {
            let value = get(name)
                .unwrap_or_else(|| format!("{default}"))
                .parse::<u64>()
                .map_err(|_| format!("{name} must be an integer"))?;
            if value == 0 || value > max {
                return Err(format!("{name} must be between 1 and {max}"));
            }
            Ok(value)
        };
        let auth = match get("AUTH_MODE").as_deref().unwrap_or("off") {
            "off" => {
                if ["AUTH_PUBLIC_KEY_PEM", "AUTH_ISSUER", "AUTH_AUDIENCE"]
                    .iter()
                    .any(|k| get(k).is_some())
                {
                    return Err("AUTH_MODE must be bearer when auth settings are present".into());
                }
                None
            }
            "bearer" => Some(crate::identity::BearerAuth::new(
                &get("AUTH_PUBLIC_KEY_PEM").ok_or("AUTH_PUBLIC_KEY_PEM is required")?,
                &get("AUTH_ISSUER").ok_or("AUTH_ISSUER is required")?,
                &get("AUTH_AUDIENCE").ok_or("AUTH_AUDIENCE is required")?,
            )?),
            _ => return Err("AUTH_MODE must be off or bearer".into()),
        };
        let mut cors_origins = Vec::new();
        if let Some(origins) = get("CORS_ORIGINS").filter(|v| !v.is_empty()) {
            for origin in origins.split(',') {
                let parsed = url::Url::parse(origin)
                    .map_err(|_| "CORS_ORIGINS must contain exact HTTP(S) origins")?;
                if !matches!(parsed.scheme(), "http" | "https")
                    || parsed.origin().ascii_serialization() != origin
                    || cors_origins.len() >= 16
                {
                    return Err("CORS_ORIGINS must contain at most 16 exact HTTP(S) origins".into());
                }
                cors_origins.push(origin.parse().map_err(|_| "CORS_ORIGINS is invalid")?);
            }
        }
        Ok(Self {
            bind,
            request_timeout: Duration::from_millis(number("REQUEST_TIMEOUT_MS", 10000, 120000)?),
            body_limit: number("BODY_LIMIT_BYTES", 16384, 1048576)? as usize,
            auth,
            cors_origins,
            anonymous_per_minute: number("RATE_ANONYMOUS_PER_MINUTE", 120, 100000)? as u32,
            authenticated_per_minute: number("RATE_AUTHENTICATED_PER_MINUTE", 60, 100000)? as u32,
            writes_per_minute: number("RATE_WRITES_PER_MINUTE", 20, 100000)? as u32,
            rate_max_keys: number("RATE_MAX_KEYS", 10000, 100000)? as usize,
            max_in_flight: number("MAX_IN_FLIGHT", 64, 10000)? as usize,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_is_validated_without_echoing_secrets() {
        let config = Config::from_lookup(|key| {
            (key == "DATABASE_URL").then(|| "postgres://u:secret@localhost/db".into())
        })
        .unwrap();
        assert_eq!(config.body_limit, 16384);
        for (key, value) in [("BODY_LIMIT_BYTES", "0"), ("BIND_ADDR", "bad")] {
            let error = Config::from_lookup(|k| {
                if k == key {
                    Some(value.into())
                } else if k == "DATABASE_URL" {
                    Some("postgres://u:secret@localhost/db".into())
                } else {
                    None
                }
            })
            .err()
            .unwrap();
            assert!(!error.contains("secret"));
        }
    }
}
