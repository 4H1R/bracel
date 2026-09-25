use super::application::validate_title;
use crate::http::error::{AppError, IssueCode, ValidationErrors};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use utoipa::ToSchema;
use uuid::Uuid;
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateNote {
    /// Whitespace is trimmed before validating 1 to 200 Unicode scalar values.
    pub title: String,
}
#[derive(Serialize, ToSchema)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    #[schema(value_type = String, format = DateTime)]
    pub created_at: sea_orm::prelude::DateTimeUtc,
}

impl From<super::entity::Model> for Note {
    fn from(model: super::entity::Model) -> Self {
        Self {
            id: model.id,
            title: model.title,
            created_at: model.created_at,
        }
    }
}

#[derive(Deserialize)]
pub struct NoteInput {
    #[serde(default)]
    title: Value,
    #[serde(flatten)]
    unknown: BTreeMap<String, serde::de::IgnoredAny>,
}

impl bracel::http::extract::Validate for NoteInput {
    type Output = CreateNote;
    fn validate(self) -> Result<CreateNote, AppError> {
        let mut errors = ValidationErrors::default();
        if !self.unknown.is_empty() {
            errors.add(
                [],
                IssueCode::UnrecognizedKeys,
                "Unknown fields are not allowed.",
            );
        }
        let title = match self.title {
            Value::String(title) => match validate_title(&title) {
                Ok(title) => title,
                Err(title_errors) => {
                    errors.merge(title_errors);
                    String::new()
                }
            },
            Value::Null => {
                errors.add(
                    ["title".into()],
                    IssueCode::InvalidType,
                    "The title field is required.",
                );
                String::new()
            }
            _ => {
                errors.add(
                    ["title".into()],
                    IssueCode::InvalidType,
                    "The title must be a string.",
                );
                String::new()
            }
        };
        errors.finish()?;
        Ok(CreateNote { title })
    }
}
