use crate::http::error::{AppError, IssueCode, ValidationErrors};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, ToSchema)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub email_verified: bool,
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
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyEmail {
    pub token: String,
}

pub enum FieldRule {
    Email,
    NewPassword,
    Name,
    Text,
}

pub trait AccountInput: serde::de::DeserializeOwned {
    const FIELDS: &'static [(&'static str, FieldRule)];
}
impl AccountInput for Registration {
    const FIELDS: &'static [(&'static str, FieldRule)] = &[
        ("email", FieldRule::Email),
        ("password", FieldRule::NewPassword),
        ("display_name", FieldRule::Name),
    ];
}
impl AccountInput for Login {
    const FIELDS: &'static [(&'static str, FieldRule)] =
        &[("email", FieldRule::Email), ("password", FieldRule::Text)];
}
impl AccountInput for Forgot {
    const FIELDS: &'static [(&'static str, FieldRule)] = &[("email", FieldRule::Email)];
}
impl AccountInput for Reset {
    const FIELDS: &'static [(&'static str, FieldRule)] = &[
        ("token", FieldRule::Text),
        ("password", FieldRule::NewPassword),
    ];
}
impl AccountInput for Profile {
    const FIELDS: &'static [(&'static str, FieldRule)] = &[("display_name", FieldRule::Name)];
}
impl AccountInput for VerifyEmail {
    const FIELDS: &'static [(&'static str, FieldRule)] = &[("token", FieldRule::Text)];
}

/// Aggregate structural and field failures before constructing the typed request.
pub fn decode<T: AccountInput>(value: serde_json::Value) -> Result<T, AppError> {
    let mut errors = ValidationErrors::default();
    let Some(object) = value.as_object() else {
        errors.add([], IssueCode::InvalidType, "Expected a JSON object.");
        return Err(errors.into());
    };
    if object
        .keys()
        .any(|key| !T::FIELDS.iter().any(|(field, _)| key == field))
    {
        errors.add(
            [],
            IssueCode::UnrecognizedKeys,
            "Unknown fields are not allowed.",
        );
    }
    for (field, rule) in T::FIELDS {
        let Some(text) = object.get(*field).and_then(serde_json::Value::as_str) else {
            errors.add(
                [(*field).into()],
                IssueCode::InvalidType,
                "This field is required and must be a string.",
            );
            continue;
        };
        let failure = match rule {
            FieldRule::Email if email(text).is_err() => Some("A valid email address is required."),
            FieldRule::NewPassword if password(text).is_err() => {
                Some("Use a password between 15 and 128 characters.")
            }
            FieldRule::Name if name(text).is_err() => {
                Some("Use a display name between 1 and 100 characters.")
            }
            _ => None,
        };
        if let Some(message) = failure {
            errors.add([(*field).into()], IssueCode::Custom, message);
        }
    }
    errors.finish()?;
    serde_json::from_value(value).map_err(|_| {
        let mut errors = ValidationErrors::default();
        errors.add([], IssueCode::Custom, "Invalid account fields.");
        AppError::from(errors)
    })
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
