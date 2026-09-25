/// Application settings; intentionally not Debug because URLs contain secrets.
#[derive(Clone)]
pub struct Config {
    pub http: bracel::config::Config,
    pub database_url: String,
    pub db_max_connections: u32,
    pub enable_example: bool,
    pub machine_tokens: bool,
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
        let http = bracel::config::Config::from_lookup(get)?;
        if machine_tokens && http.auth.is_none() {
            return Err("AUTH_MACHINE_TOKENS requires AUTH_MODE=bearer".into());
        }
        Ok(Self {
            http,
            machine_tokens,
            database_url,
            db_max_connections,
            enable_example,
        })
    }
}
