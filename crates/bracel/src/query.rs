//! Allowlisted collection queries. Features supply columns and an already scoped Select.
use crate::http::{
    error::{AppError, IssueCode, ValidationErrors},
    pagination::PageQuery,
};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, Order, QueryFilter, QueryOrder, Select,
    prelude::DateTimeUtc,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub enum FilterKind {
    Uuid,
    TextExact,
    TextContains,
    TimestampFrom,
    TimestampTo,
}

pub struct Filter<C> {
    pub name: &'static str,
    pub column: C,
    pub kind: FilterKind,
}

/// Each sort is a complete non-null, immutable key ending in a unique column.
/// All columns use the requested direction. Features own typed cursor positions.
pub struct Sort<C> {
    pub name: &'static str,
    pub columns: Vec<C>,
}
pub struct QuerySpec<C> {
    pub resource: &'static str,
    pub filters: Vec<Filter<C>>,
    pub sorts: Vec<Sort<C>>,
    pub default_sort: &'static str,
}

pub struct CollectionQuery<C> {
    condition: Condition,
    columns: Vec<C>,
    pub descending: bool,
    pub page: PageQuery,
    pub scope: String,
}

pub fn timestamp(value: &str) -> Option<DateTimeUtc> {
    let date = value.parse::<DateTimeUtc>().ok()?;
    is_supported_timestamp(&date).then_some(date)
}

/// PostgreSQL-compatible microseconds, without leap seconds, in years 1 through 9999.
pub fn is_supported_timestamp(date: &DateTimeUtc) -> bool {
    date.timestamp_subsec_nanos() < 1_000_000_000
        && date.timestamp_subsec_nanos().is_multiple_of(1000)
        && (-62_135_596_800_000_000..=253_402_300_799_999_999).contains(&date.timestamp_micros())
}

fn invalid(field: &str) -> AppError {
    let mut errors = ValidationErrors::default();
    errors.add([field.into()], IssueCode::Custom, "Invalid filter value.");
    errors.into()
}

impl<C: ColumnTrait> QuerySpec<C> {
    pub fn parse(
        &self,
        mut fields: BTreeMap<String, String>,
        access_scope: &str,
    ) -> Result<CollectionQuery<C>, AppError> {
        let bad = || {
            AppError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "Unsupported collection query",
            )
        };
        let limit = fields
            .remove("limit")
            .map(|v| v.parse::<u64>().map_err(|_| bad()))
            .transpose()?;
        let after = fields.remove("after");
        let sort = fields
            .remove("sort")
            .unwrap_or_else(|| self.default_sort.into());
        let descending = sort.starts_with('-');
        let name = sort.strip_prefix('-').unwrap_or(&sort);
        let sorting = self.sorts.iter().find(|s| s.name == name).ok_or_else(bad)?;
        let mut condition = Condition::all();
        let mut canonical = BTreeMap::new();
        for (key, value) in fields {
            let name = key
                .strip_prefix("filter[")
                .and_then(|v| v.strip_suffix(']'))
                .ok_or_else(bad)?;
            let filter = self
                .filters
                .iter()
                .find(|f| f.name == name)
                .ok_or_else(bad)?;
            if value.is_empty() || value.chars().count() > 200 {
                return Err(invalid(&key));
            }
            let (expression, normalized) = match filter.kind {
                FilterKind::Uuid => {
                    let id = value.parse::<uuid::Uuid>().map_err(|_| invalid(&key))?;
                    (filter.column.eq(id), id.to_string())
                }
                FilterKind::TextExact => (filter.column.eq(value.clone()), value),
                FilterKind::TextContains => {
                    // PostgreSQL LIKE uses backslash escaping; wildcards are literal input.
                    let escaped = value
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_");
                    (filter.column.like(format!("%{escaped}%")), value)
                }
                FilterKind::TimestampFrom | FilterKind::TimestampTo => {
                    let date = timestamp(&value).ok_or_else(|| invalid(&key))?;
                    let expression = if matches!(filter.kind, FilterKind::TimestampFrom) {
                        filter.column.gte(date)
                    } else {
                        filter.column.lte(date)
                    };
                    (expression, date.to_rfc3339())
                }
            };
            condition = condition.add(expression);
            canonical.insert(key, normalized);
        }
        // Stable encoding, independent of parameter order, binds navigation to scope.
        let binding = serde_json::to_vec(&(self.resource, access_scope, sort, canonical))
            .expect("query binding");
        let scope = format!("{}:{:x}", self.resource, Sha256::digest(binding));
        Ok(CollectionQuery {
            condition,
            columns: sorting.columns.clone(),
            descending,
            page: PageQuery { limit, after },
            scope,
        })
    }

    /// OpenAPI query parameters generated from the filter and sort declarations.
    pub fn parameters(&self) -> Vec<Value> {
        let mut params = vec![
            json!({"name":"sort","in":"query","required":false,"description":"Stable order with a unique tie-breaker","schema":{"type":"string","default":self.default_sort,"enum":self.sorts.iter().flat_map(|s| [s.name.to_owned(), format!("-{}",s.name)]).collect::<Vec<_>>()}}),
        ];
        for filter in &self.filters {
            let (format, description) = match filter.kind {
                FilterKind::Uuid => (Some("uuid"), "Exact UUID"),
                FilterKind::TextExact => (None, "Exact, case-sensitive text"),
                FilterKind::TextContains => (None, "Case-sensitive substring; % and _ are literal"),
                FilterKind::TimestampFrom => (
                    Some("date-time"),
                    "Inclusive lower bound; microsecond precision",
                ),
                FilterKind::TimestampTo => (
                    Some("date-time"),
                    "Inclusive upper bound; microsecond precision",
                ),
            };
            let mut schema = json!({"type":"string","minLength":1,"maxLength":200});
            if let Some(format) = format {
                schema["format"] = json!(format);
            }
            params.push(json!({"name":format!("filter[{}]",filter.name),"in":"query","required":false,"description":description,"schema":schema}));
        }
        params
    }
}

impl<C: ColumnTrait> CollectionQuery<C> {
    pub fn apply<E: EntityTrait<Column = C>>(&self, query: Select<E>) -> Select<E> {
        let mut query = query.filter(self.condition.clone());
        sea_orm::QuerySelect::query(&mut query).clear_order_by();
        for column in &self.columns {
            query = query.order_by(
                *column,
                if self.descending {
                    Order::Desc
                } else {
                    Order::Asc
                },
            );
        }
        query
    }
}
