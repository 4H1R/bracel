//! Bounded collection navigation and response projection; applications own authorized queries.
use super::{
    error::AppError,
    pagination::{PageQuery, PageRequest, encode_cursor},
};
use axum::http::StatusCode;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub fn binding<T: Serialize>(resource: &str, context: &T) -> Result<String, AppError> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(context).map_err(|_| {
        AppError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Could not bind collection context",
        )
    })?;
    Ok(format!("{resource}:{:x}", Sha256::digest(bytes)))
}

pub struct Selection {
    pub fields: BTreeSet<String>,
    pub includes: BTreeSet<String>,
}
impl Selection {
    pub fn parse(
        query: &mut BTreeMap<String, String>,
        fields: &[&str],
        includes: &[&str],
    ) -> Result<Self, AppError> {
        fn parse(raw: Option<String>, allowed: &[&str]) -> Result<BTreeSet<String>, AppError> {
            let Some(raw) = raw else {
                return Ok(BTreeSet::new());
            };
            let selected = raw.split(',').map(str::to_owned).collect::<BTreeSet<_>>();
            if selected.len() > 16
                || selected
                    .iter()
                    .any(|value| !allowed.contains(&value.as_str()))
            {
                return Err(AppError::new(
                    StatusCode::BAD_REQUEST,
                    "Unsupported fields or includes",
                ));
            }
            Ok(selected)
        }
        Ok(Self {
            fields: parse(query.remove("fields"), fields)?,
            includes: parse(query.remove("include"), includes)?,
        })
    }
    pub fn project(&self, mut dto: Value) -> Value {
        if !self.fields.is_empty()
            && let Some(object) = dto.as_object_mut()
        {
            object.retain(|key, _| self.fields.contains(key) || self.includes.contains(key));
        }
        dto
    }
}

pub struct Navigation<C> {
    pub limit: u64,
    pub position: Option<C>,
    pub backward: bool,
    pub offset: Option<u64>,
}
impl<C: DeserializeOwned> Navigation<C> {
    pub fn parse(query: &mut BTreeMap<String, String>, scope: &str) -> Result<Self, AppError> {
        let bad = || AppError::new(StatusCode::BAD_REQUEST, "Invalid pagination options");
        let before = query.remove("before");
        let after = query.remove("after");
        let offset = query
            .remove("offset")
            .map(|s| s.parse::<u64>().map_err(|_| bad()))
            .transpose()?;
        if before.is_some() && after.is_some()
            || offset.is_some_and(|n| n > 10000)
            || offset.is_some() && (before.is_some() || after.is_some())
        {
            return Err(bad());
        }
        let backward = before.is_some();
        let limit = query
            .remove("limit")
            .map(|s| s.parse::<u64>().map_err(|_| bad()))
            .transpose()?;
        let page = PageRequest::parse(
            PageQuery {
                limit,
                after: before.or(after),
            },
            scope,
        )?;
        Ok(Self {
            limit: page.limit(),
            position: page.into_position(),
            backward,
            offset,
        })
    }
}
pub fn cursor<C: Serialize>(position: C, scope: &str) -> Result<String, AppError> {
    encode_cursor(position, scope)
}
