//! Validated provider settings. Secrets are intentionally not Debug or Serialize.
use std::path::PathBuf;

#[derive(Clone)]
#[cfg_attr(not(any(feature = "mail", feature = "batteries")), allow(dead_code))]
enum Smtp {
    Local(u16),
    Relay {
        host: String,
        username: String,
        password: String,
    },
}

#[derive(Clone)]
pub struct MailSettings {
    transport: Option<Smtp>,
    pub from: String,
}
impl Default for MailSettings {
    fn default() -> Self {
        Self {
            transport: None,
            from: "Bracel <noreply@example.test>".into(),
        }
    }
}
impl MailSettings {
    pub fn from_lookup(get: &impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let transport = match (get("MAIL_LOCAL_PORT"), get("MAIL_SMTP_HOST")) {
            (Some(_), Some(_)) => return Err("Select MAIL_LOCAL_PORT or MAIL_SMTP_HOST".into()),
            (Some(port), None) => Some(Smtp::Local(
                port.parse::<u16>()
                    .ok()
                    .filter(|p| *p > 0)
                    .ok_or("Invalid SMTP port")?,
            )),
            (None, Some(host)) => {
                if host.is_empty() {
                    return Err("MAIL_SMTP_HOST cannot be empty".into());
                }
                Some(Smtp::Relay {
                    host,
                    username: get("MAIL_SMTP_USERNAME").ok_or("SMTP username required")?,
                    password: get("MAIL_SMTP_PASSWORD").ok_or("SMTP password required")?,
                })
            }
            _ => None,
        };
        let settings = Self {
            transport,
            from: get("MAIL_FROM").unwrap_or_else(|| Self::default().from),
        };
        #[cfg(any(feature = "mail", feature = "batteries"))]
        if settings.configured() {
            bracel_integrations::mail::message(
                &settings.from,
                "validation@example.test",
                "Validation",
                "Validation",
            )
            .map_err(|_| "MAIL_FROM must be a valid mailbox")?;
        }
        Ok(settings)
    }
    pub fn configured(&self) -> bool {
        self.transport.is_some()
    }
    #[cfg(any(feature = "mail", feature = "batteries"))]
    pub fn build(&self) -> Result<bracel_integrations::mail::Mailer, &'static str> {
        use bracel_integrations::mail::Mailer;
        match &self.transport {
            Some(Smtp::Local(port)) => Ok(Mailer::local(*port)),
            Some(Smtp::Relay {
                host,
                username,
                password,
            }) => Mailer::relay(host, username.clone(), password.clone())
                .map_err(|_| "Invalid SMTP configuration"),
            None => Err("Configure SMTP before running the mail worker"),
        }
    }
}

#[derive(Clone, Default)]
pub struct Providers {
    pub mail: MailSettings,
    pub files_root: Option<PathBuf>,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
    pub incoming_secret: Option<String>,
    pub notify_email: Option<String>,
    pub discovery_url: Option<String>,
    pub telemetry_endpoint: Option<String>,
}
impl Providers {
    pub fn from_lookup(get: &impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let settings = Self {
            mail: MailSettings::from_lookup(get)?,
            files_root: get("FILES_ROOT").map(PathBuf::from),
            webhook_url: get("WEBHOOK_URL"),
            webhook_secret: get("WEBHOOK_SECRET"),
            incoming_secret: get("WEBHOOK_INCOMING_SECRET"),
            notify_email: get("NOTIFY_EMAIL"),
            discovery_url: get("AUTH_DISCOVERY_URL"),
            telemetry_endpoint: get("OTLP_ENDPOINT"),
        };
        for (name, value) in [
            ("WEBHOOK_URL", &settings.webhook_url),
            ("AUTH_DISCOVERY_URL", &settings.discovery_url),
            ("OTLP_ENDPOINT", &settings.telemetry_endpoint),
        ] {
            if let Some(value) = value {
                let url = url::Url::parse(value).map_err(|_| format!("Invalid {name}"))?;
                if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
                    return Err(format!("Invalid {name}"));
                }
            }
        }
        if settings.discovery_url.is_some() && settings.discovery_url != get("AUTH_ISSUER") {
            return Err("AUTH_DISCOVERY_URL must match the configured issuer".into());
        }
        Ok(settings)
    }
}
