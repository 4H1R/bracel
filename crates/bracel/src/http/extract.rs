use super::error::{AppError, IssueCode, ValidationErrors};
use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Query, Request, rejection::JsonRejection},
    http::{StatusCode, request::Parts},
};
use serde::de::DeserializeOwned;

/// Convert transport input to a validated application command before entering a handler.
pub trait Validate {
    type Output;
    fn validate(self) -> Result<Self::Output, AppError>;
}

pub struct ValidatedJson<T: Validate>(pub T::Output);
impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate + Send,
{
    type Rejection = AppError;
    async fn from_request(request: Request, state: &S) -> Result<Self, AppError> {
        let Json(input) =
            Json::<T>::from_request(request, state)
                .await
                .map_err(|error| match error {
                    JsonRejection::JsonDataError(_) => {
                        let mut errors = ValidationErrors::default();
                        errors.add(
                            [],
                            IssueCode::Custom,
                            "Expected an object with no duplicate fields.",
                        );
                        AppError::from(errors)
                    }
                    _ => AppError::new(error.status(), "Invalid JSON request"),
                })?;
        input.validate().map(Self)
    }
}

pub struct ValidatedQuery<T: Validate>(pub T::Output);
impl<S, T> FromRequestParts<S> for ValidatedQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate + Send,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let Query(input) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "Invalid query parameters"))?;
        input.validate().map(Self)
    }
}

pub struct TypedPath<T>(pub T);
impl<S, T> FromRequestParts<S> for TypedPath<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "Invalid path parameters"))
    }
}
