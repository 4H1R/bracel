use crate::http::error::{AppError, IssueCode, ValidationErrors};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

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

#[derive(Serialize, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
}
#[derive(Serialize, ToSchema)]
pub struct Session {
    pub user: User,
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i32,
}
#[derive(Serialize, ToSchema)]
pub struct Accepted {
    pub message: &'static str,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub email: String,
    pub password: String,
    pub display_name: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Login {
    pub email: String,
    pub password: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Forgot {
    pub email: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Reset {
    pub token: String,
    pub password: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub display_name: String,
}
pub fn invalid(field: &str, message: &'static str) -> AppError {
    let mut errors = ValidationErrors::default();
    errors.add([field.into()], IssueCode::Custom, message);
    errors.finish().expect_err("one validation issue")
}
pub fn email(value: &str) -> Result<String, AppError> {
    let value = value.trim().to_ascii_lowercase();
    let parts = value.split('@').collect::<Vec<_>>();
    if value.len() > 254
        || parts.len() != 2
        || parts[0].is_empty()
        || parts[0].len() > 64
        || !parts[0]
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&c))
        || parts[0].starts_with('.')
        || parts[0].ends_with('.')
        || parts[0].contains("..")
        || !parts[1].contains('.')
        || parts[1].split('.').any(|p| {
            p.is_empty()
                || p.len() > 63
                || p.starts_with('-')
                || p.ends_with('-')
                || !p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
    {
        return Err(invalid("email", "A valid email address is required."));
    }
    Ok(value)
}
pub fn password(value: &str) -> Result<(), AppError> {
    if !(15..=128).contains(&value.chars().count()) || value.len() > 512 {
        return Err(invalid(
            "password",
            "Use a password between 15 and 128 characters.",
        ));
    }
    Ok(())
}
pub fn name(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 100 || value.chars().any(char::is_control) {
        return Err(invalid(
            "display_name",
            "Use a display name between 1 and 100 characters.",
        ));
    }
    Ok(value.into())
}
