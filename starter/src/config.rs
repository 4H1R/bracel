/// Application settings; intentionally not Debug because URLs contain secrets.
#[derive(Clone)]
pub struct Config {
    pub providers: crate::provider_settings::Providers,
    #[cfg(feature = "cache")]
    pub cache: crate::cache::Settings,
    pub accounts: crate::features::accounts::Settings,
    pub local_auth: bool,
    pub http: bracel::config::Config,
    pub database_url: String,
    pub db_max_connections: u32,
    pub enable_example: bool,
    pub machine_tokens: bool,
    pub enable_batteries: bool,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let database_url = get("DATABASE_URL").ok_or("DATABASE_URL is required")?;
        let url =
            url::Url::parse(&database_url).map_err(|_| "DATABASE_URL must be a PostgreSQL URL")?;
        if !matches!(url.scheme(), "postgres" | "postgresql") || url.host_str().is_none() {
            return Err("DATABASE_URL must be a PostgreSQL URL".into());
        }
        let db_max_connections = get("DB_MAX_CONNECTIONS")
            .unwrap_or_else(|| "10".into())
            .parse::<u32>()
            .map_err(|_| "DB_MAX_CONNECTIONS must be an integer")?;
        if !(1..=100).contains(&db_max_connections) {
            return Err("DB_MAX_CONNECTIONS must be between 1 and 100".into());
        }
        let enable_example = get("ENABLE_EXAMPLE")
            .unwrap_or_else(|| "false".into())
            .parse()
            .map_err(|_| "ENABLE_EXAMPLE must be true or false")?;
        let machine_tokens = get("AUTH_MACHINE_TOKENS")
            .unwrap_or_else(|| "false".into())
            .parse::<bool>()
            .map_err(|_| "AUTH_MACHINE_TOKENS must be true or false")?;
        let enable_batteries = get("ENABLE_BATTERIES")
            .unwrap_or_else(|| "false".into())
            .parse::<bool>()
            .map_err(|_| "ENABLE_BATTERIES must be true or false")?;
        if get("AUTH_DISCOVERY_URL").is_some() && !cfg!(feature = "identity") {
            return Err("AUTH_DISCOVERY_URL requires the identity feature".into());
        }
        let accounts = crate::features::accounts::Settings::from_lookup(&get)?;
        let local_auth = get("AUTH_MODE").as_deref().unwrap_or("local") == "local";
        if local_auth && !accounts.enabled {
            return Err("AUTH_MODE=local requires ENABLE_ACCOUNTS=true".into());
        }
        let mut http = bracel::config::Config::from_lookup(|key| {
            if key == "AUTH_MODE" && local_auth {
                Some("off".into())
            } else {
                get(key)
            }
        })?;
        if local_auth {
            http.auth = Some(bracel::identity::BearerAuth::pending(
                crate::features::accounts::ISSUER,
                "starter-api",
            )?);
        }
        http.middleware = crate::middleware::ENABLED.to_vec();
        if enable_batteries && (!cfg!(feature = "batteries") || http.auth.is_none()) {
            return Err(
                "ENABLE_BATTERIES requires the batteries feature and bearer authentication".into(),
            );
        }
        if machine_tokens && http.auth.is_none() {
            return Err("AUTH_MACHINE_TOKENS requires AUTH_MODE=bearer".into());
        }
        Ok(Self {
            providers: crate::provider_settings::Providers::from_lookup(&get)?,
            #[cfg(feature = "cache")]
            cache: crate::cache::Settings::from_lookup(&get)?,
            accounts,
            local_auth,
            http,
            machine_tokens,
            enable_batteries,
            database_url,
            db_max_connections,
            enable_example,
        })
    }
}
