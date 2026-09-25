use super::{
    dto::{CreateNote, Note},
    entity,
};
use crate::http::error::{AppError, IssueCode, ValidationErrors};
use crate::http::pagination::{PageRequest, invalid_cursor};
use crate::query::CollectionQuery;
use sea_orm::{ActiveModelTrait, ConnectionTrait, EntityTrait, Set};
use sea_orm::{
    QueryFilter, QuerySelect,
    prelude::DateTimeUtc,
    sea_query::{Expr, ExprTrait},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
pub fn validate_title(title: &str) -> Result<String, ValidationErrors> {
    let title = title.trim();
    let issue = if title.is_empty() {
        Some((IssueCode::TooSmall, "The title field is required."))
    } else if title.chars().count() > 200 {
        Some((
            IssueCode::TooBig,
            "The title must not be greater than 200 characters.",
        ))
    } else {
        None
    };
    if let Some((code, message)) = issue {
        let mut errors = ValidationErrors::default();
        errors.add(["title".into()], code, message);
        return Err(errors);
    }
    Ok(title.into())
}

pub async fn create_note(db: &impl ConnectionTrait, input: CreateNote) -> Result<Note, AppError> {
    let title = validate_title(&input.title)?;
    let model = entity::ActiveModel {
        id: Set(Uuid::now_v7()),
        title: Set(title),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(model.into())
}

pub async fn get_note(db: &impl ConnectionTrait, id: Uuid) -> Result<Note, AppError> {
    entity::Entity::find_by_id(id)
        .one(db)
        .await?
        .map(Into::into)
        .ok_or_else(|| AppError::new(axum::http::StatusCode::NOT_FOUND, "Note not found"))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteCursor {
    created_at: DateTimeUtc,
    id: Uuid,
}

pub struct ListNotes {
    page: PageRequest<NoteCursor>,
    query: CollectionQuery<entity::Column>,
}

impl ListNotes {
    pub fn parse(raw: &str, access_scope: &str) -> Result<Self, AppError> {
        let mut query =
            super::query::spec().parse(crate::http::query::decode(raw)?, access_scope)?;
        let request =
            PageRequest::<NoteCursor>::parse(std::mem::take(&mut query.page), &query.scope)?;
        if let Some(position) = request.after() {
            // PostgreSQL stores microseconds. Bound client timestamps to years
            // 1..=9999 so invalid database timestamps remain a client error.
            if position.created_at.timestamp_subsec_nanos() >= 1_000_000_000
                || position.created_at.timestamp_subsec_nanos() % 1000 != 0
                || !(-62_135_596_800_000_000..=253_402_300_799_999_999)
                    .contains(&position.created_at.timestamp_micros())
            {
                return Err(invalid_cursor());
            }
        }
        Ok(Self {
            page: request,
            query,
        })
    }

    pub fn limit(&self) -> u64 {
        self.page.limit()
    }
    pub fn cursor_scope(&self) -> &str {
        &self.query.scope
    }
}

pub struct ListedNotes {
    pub items: Vec<Note>,
    pub next: Option<NoteCursor>,
}

pub async fn list_notes(
    db: &impl ConnectionTrait,
    input: &ListNotes,
) -> Result<ListedNotes, AppError> {
    let mut query = input.query.apply(entity::Entity::find());
    if let Some(position) = input.page.after() {
        let columns = Expr::tuple([
            Expr::col(entity::Column::CreatedAt),
            Expr::col(entity::Column::Id),
        ]);
        let values = Expr::tuple([Expr::val(position.created_at), Expr::val(position.id)]);
        query = query.filter(if input.query.descending {
            columns.lt(values)
        } else {
            columns.gt(values)
        });
    }
    let mut rows = query.limit(input.limit() + 1).all(db).await?;
    let has_more = rows.len() > input.limit() as usize;
    rows.truncate(input.limit() as usize);
    let next = if has_more {
        rows.last().map(|row| NoteCursor {
            created_at: row.created_at,
            id: row.id,
        })
    } else {
        None
    };
    Ok(ListedNotes {
        items: rows.into_iter().map(Into::into).collect(),
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn title_boundaries() {
        assert_eq!(validate_title(" hi ").ok().unwrap(), "hi");
        assert!(validate_title(" \n").is_err());
        assert!(validate_title(&"é".repeat(200)).is_ok());
        assert!(validate_title(&"a".repeat(201)).is_err());
    }
}
