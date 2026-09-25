use crate::{
    http::error::AppError,
    identity::{BearerAuth, Principal},
};
use axum::{
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};

pub(super) enum AuthError {
    Unauthorized,
    Forbidden,
    Unavailable,
}
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, challenge, detail) = match self {
            Self::Unavailable => {
                return AppError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Authentication unavailable",
                )
                .into_response();
            }
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "Bearer",
                "A valid bearer access token is required",
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "Bearer error=\"insufficient_scope\"",
                "Required permission is missing",
            ),
        };
        (
            [(header::WWW_AUTHENTICATE, challenge)],
            AppError::new(status, detail),
        )
            .into_response()
    }
}

pub(super) async fn authenticate(
    request: &HeaderMap,
    verifier: &BearerAuth,
    scope: &str,
) -> Result<Principal, AuthError> {
    let mut headers = request.get_all(header::AUTHORIZATION).iter();
    let value = headers
        .next()
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::Unauthorized)?;
    if headers.next().is_some() {
        return Err(AuthError::Unauthorized);
    }
    let (scheme, token) = value.split_once(' ').ok_or(AuthError::Unauthorized)?;
    if !scheme.eq_ignore_ascii_case("Bearer")
        || token.is_empty()
        || token.contains(char::is_whitespace)
    {
        return Err(AuthError::Unauthorized);
    }
    let principal = verifier
        .verify_access(token)
        .await
        .map_err(|_| AuthError::Unavailable)?
        .ok_or(AuthError::Unauthorized)?;
    if !principal.allows(scope) {
        return Err(AuthError::Forbidden);
    }
    Ok(principal)
}
