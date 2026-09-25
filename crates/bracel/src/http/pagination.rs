//! Bounded, versioned cursors. Features supply query scope and typed positions.
use super::error::{AppError, IssueCode, ValidationErrors};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use utoipa::IntoParams;

pub const DEFAULT_LIMIT: u64 = 25;
pub const MAX_LIMIT: u64 = 100;
pub const MAX_CURSOR_BYTES: usize = 1024;

#[derive(Default, Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    /// Page size, from 1 through 100. Defaults to 25.
    #[param(minimum = 1, maximum = 100, default = 25)]
    pub limit: Option<u64>,
    /// Opaque cursor from the previous page, at most 1024 bytes.
    #[param(max_length = 1024)]
    pub after: Option<String>,
}

pub struct PageRequest<C> {
    limit: u64,
    after: Option<C>,
}

impl<C: DeserializeOwned> PageRequest<C> {
    pub fn parse(query: PageQuery, scope: &str) -> Result<Self, AppError> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        if !(1..=MAX_LIMIT).contains(&limit) {
            let mut errors = ValidationErrors::default();
            errors.add(
                ["limit".into()],
                IssueCode::Custom,
                "Limit must be between 1 and 100.",
            );
            return Err(errors.into());
        }
        let after = query
            .after
            .map(|token| decode_cursor(&token, scope))
            .transpose()?;
        Ok(Self { limit, after })
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }
    pub fn after(&self) -> Option<&C> {
        self.after.as_ref()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor<C> {
    version: u8,
    scope: String,
    position: C,
}

pub fn invalid_cursor() -> AppError {
    let mut errors = ValidationErrors::default();
    errors.add(
        ["after".into()],
        IssueCode::Custom,
        "Invalid cursor for this collection.",
    );
    errors.into()
}

fn decode_cursor<C: DeserializeOwned>(token: &str, scope: &str) -> Result<C, AppError> {
    if token.is_empty() || token.len() > MAX_CURSOR_BYTES {
        return Err(invalid_cursor());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| invalid_cursor())?;
    let cursor: Cursor<C> = serde_json::from_slice(&bytes).map_err(|_| invalid_cursor())?;
    if cursor.version != 1 || cursor.scope != scope {
        return Err(invalid_cursor());
    }
    Ok(cursor.position)
}

pub fn encode_cursor<C: Serialize>(position: C, scope: &str) -> Result<String, AppError> {
    let failure = || {
        AppError::new(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Could not create page cursor",
        )
    };
    let bytes = serde_json::to_vec(&Cursor {
        version: 1,
        scope: scope.into(),
        position,
    })
    .map_err(|_| failure())?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    if token.len() > MAX_CURSOR_BYTES {
        return Err(failure());
    }
    Ok(token)
}
