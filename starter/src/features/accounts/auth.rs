use super::{SCOPE, application, dto::User};
use crate::{AppState, http::error::AppError};
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use bracel::identity::Principal;

/// The local user behind a verified request. Use on a scope-protected route.
/// External JWT identities are not interpreted as local database users.
pub struct CurrentUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let principal = Principal::from_request_parts(parts, state).await?;
        if !principal.allows(SCOPE) {
            return Err(
                AppError::new(StatusCode::FORBIDDEN, "Required permission is missing")
                    .into_response(),
            );
        }
        application::me(&state.db, &principal)
            .await
            .map(Self)
            .map_err(IntoResponse::into_response)
    }
}
