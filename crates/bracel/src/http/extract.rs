use super::error::{AppError, IssueCode, ValidationErrors};
use axum::{
    Json,
    extract::{FromRequest, FromRequestParts, Path, Query, Request, rejection::JsonRejection},
    http::{StatusCode, request::Parts},
};
use serde::de::DeserializeOwned;

/// JSON value whose object keys are unique at every nesting level.
pub struct UniqueJson(pub serde_json::Value);
impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = serde_json::Value;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| E::custom("Invalid number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(serde_json::Value::Null)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(v)) = seq.next_element()? {
                    values.push(v);
                }
                Ok(values.into())
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("Duplicate object key"));
                    }
                    values.insert(key, map.next_value::<UniqueJson>()?.0);
                }
                Ok(values.into())
            }
        }
        deserializer.deserialize_any(Visitor).map(Self)
    }
}
impl<S: Send + Sync> FromRequest<S> for UniqueJson {
    type Rejection = AppError;
    async fn from_request(request: Request, state: &S) -> Result<Self, AppError> {
        Json::<Self>::from_request(request, state)
            .await
            .map(|Json(v)| v)
            .map_err(|error| AppError::new(error.status(), "Invalid JSON or duplicate object key"))
    }
}

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
