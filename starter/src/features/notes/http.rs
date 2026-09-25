use super::{
    application,
    dto::{CreateNote, Note, NoteInput},
};
use crate::http::{
    pagination::{PageQuery, encode_cursor},
    response::Page,
};
use crate::{
    AppState,
    http::{
        error::{AppError, IssueCode, ValidationErrors},
        response::Data,
    },
};
use axum::{Extension, extract::RawQuery};
use axum::{
    Json, Router,
    extract::{
        Path, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
    routing::get,
};
use uuid::Uuid;
pub(crate) const READ_SCOPE: &str = "notes:read";
pub(crate) const WRITE_SCOPE: &str = "notes:write";

pub(crate) fn routes(
    policies: &crate::http::middleware::Policies,
    protected: bool,
) -> Router<AppState> {
    use crate::http::middleware::Access;
    let reads = Router::new()
        .route("/example/notes", get(list))
        .route("/example/notes/{id}", get(get_note));
    let writes = Router::new().route("/example/notes", axum::routing::post(create));
    policies
        .apply(
            reads,
            if protected {
                Access::Scope(READ_SCOPE)
            } else {
                Access::Public
            },
        )
        .merge(policies.apply(
            writes,
            if protected {
                Access::Scope(WRITE_SCOPE)
            } else {
                Access::Public
            },
        ))
}
#[utoipa::path(post, path = "/example/notes", request_body = CreateNote, responses(
    (status = 201, description = "Created note", body = Data<Note>),
    (status = 400, description = "Malformed JSON", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 413, description = "Body too large", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 415, description = "Expected JSON", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 422, description = "Invalid note", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 408, description = "Deadline exceeded", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 503, description = "Database unavailable", body = crate::http::error::Problem, content_type = "application/problem+json")
))]
pub async fn create(
    State(state): State<AppState>,
    input: Result<Json<NoteInput>, JsonRejection>,
) -> Result<(StatusCode, Json<Data<Note>>), AppError> {
    let Json(input) = input.map_err(|error| match error {
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
    let input = input.validate()?;
    Ok((
        StatusCode::CREATED,
        Json(Data::new(application::create_note(&state.db, input).await?)),
    ))
}

#[utoipa::path(get, path = "/example/notes/{id}", params(("id" = Uuid, Path, description = "Note ID")), responses(
    (status = 200, description = "Note", body = Data<Note>),
    (status = 400, description = "Invalid ID", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 404, description = "Missing note", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 408, description = "Deadline exceeded", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 503, description = "Database unavailable", body = crate::http::error::Problem, content_type = "application/problem+json")
))]
pub async fn get_note(
    State(state): State<AppState>,
    id: Result<Path<Uuid>, PathRejection>,
) -> Result<Json<Data<Note>>, AppError> {
    let Path(id) = id.map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "id must be a UUID"))?;
    Ok(Json(Data::new(application::get_note(&state.db, id).await?)))
}

#[utoipa::path(get, path = "/example/notes", params(PageQuery), responses(
    (status = 200, description = "Notes ordered by creation time and ID descending", body = Page<Note>),
    (status = 400, description = "Malformed or unsupported query parameters", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 422, description = "Invalid limit or cursor", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 408, description = "Deadline exceeded", body = crate::http::error::Problem, content_type = "application/problem+json"),
    (status = 503, description = "Database unavailable", body = crate::http::error::Problem, content_type = "application/problem+json")
))]
pub async fn list(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    principal: Option<Extension<crate::features::identity::Principal>>,
) -> Result<Json<Page<Note>>, AppError> {
    let access_scope = principal
        .as_ref()
        .map(|p| p.0.cursor_scope())
        .unwrap_or_else(|| "public".into());
    let input = application::ListNotes::parse(query.as_deref().unwrap_or(""), &access_scope)?;
    let page = application::list_notes(&state.db, &input).await?;
    let next = page
        .next
        .map(|position| encode_cursor(position, input.cursor_scope()))
        .transpose()?;
    Ok(Json(Page::new(page.items, input.limit(), next)))
}
