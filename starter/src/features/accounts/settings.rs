/// Settings for the local accounts feature.
#[derive(Clone)]
pub struct Settings {
    pub enabled: bool,
    pub registration: bool,
    pub mail_configured: bool,
}

impl Settings {
    pub fn from_lookup(get: &impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let boolean = |key: &str| -> Result<bool, String> {
            get(key)
                .unwrap_or_else(|| "true".into())
                .parse()
                .map_err(|_| format!("{key} must be true or false"))
        };
        let local = get("MAIL_LOCAL_PORT");
        if let Some(port) = &local {
            port.parse::<u16>()
                .ok()
                .filter(|p| *p > 0)
                .ok_or("MAIL_LOCAL_PORT must be a valid port")?;
        }
        let host = get("MAIL_SMTP_HOST");
        if local.is_some() && host.is_some() {
            return Err("Select MAIL_LOCAL_PORT or MAIL_SMTP_HOST".into());
        }
        if host.as_ref().is_some_and(|h| h.is_empty()) {
            return Err("MAIL_SMTP_HOST cannot be empty".into());
        }
        if host.is_some()
            && (get("MAIL_SMTP_USERNAME").is_none() || get("MAIL_SMTP_PASSWORD").is_none())
        {
            return Err("SMTP relay requires MAIL_SMTP_USERNAME and MAIL_SMTP_PASSWORD".into());
        }
        let mail_configured = cfg!(feature = "mail") && (local.is_some() || host.is_some());
        #[cfg(feature = "mail")]
        if mail_configured {
            let from = get("MAIL_FROM").unwrap_or_else(|| "Bracel <noreply@example.test>".into());
            bracel_integrations::mail::message(&from, "validation@example.test", "Reset", "Reset")
                .map_err(|_| "MAIL_FROM must be a valid mailbox")?;
        }
        Ok(Self {
            enabled: boolean("ENABLE_ACCOUNTS")?,
            registration: boolean("ALLOW_REGISTRATION")?,
            mail_configured,
        })
    }
}
