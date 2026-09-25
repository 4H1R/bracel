use axum::{
    Extension, Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct Problem {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    /// Server-generated correlation ID, also returned in X-Request-ID.
    pub request_id: String,
    /// Structured validation issues; an empty path identifies the whole request.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ValidationIssue>,
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(untagged)]
pub enum PathSegment {
    Field(String),
    Index(usize),
}

impl From<&str> for PathSegment {
    fn from(field: &str) -> Self {
        Self::Field(field.into())
    }
}

impl From<usize> for PathSegment {
    fn from(index: usize) -> Self {
        Self::Index(index)
    }
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    InvalidType,
    TooSmall,
    TooBig,
    UnrecognizedKeys,
    Custom,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct ValidationIssue {
    pub code: IssueCode,
    pub path: Vec<PathSegment>,
    pub message: String,
}

#[derive(Clone, Default)]
pub struct ValidationErrors(Vec<ValidationIssue>);

impl ValidationErrors {
    pub fn add(
        &mut self,
        path: impl IntoIterator<Item = PathSegment>,
        code: IssueCode,
        message: impl Into<String>,
    ) {
        self.0.push(ValidationIssue {
            code,
            path: path.into_iter().collect(),
            message: message.into(),
        });
    }

    pub fn merge(&mut self, other: Self) {
        self.0.extend(other.0);
    }

    pub fn finish(self) -> Result<(), AppError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(self.into())
        }
    }
}

#[derive(Clone)]
pub struct AppError {
    status: StatusCode,
    detail: &'static str,
    issues: Vec<ValidationIssue>,
}

impl AppError {
    pub fn new(status: StatusCode, detail: &'static str) -> Self {
        Self {
            status,
            detail,
            issues: Vec::new(),
        }
    }
}

impl From<ValidationErrors> for AppError {
    fn from(errors: ValidationErrors) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            detail: "The given data was invalid.",
            issues: errors.0,
        }
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(_: sea_orm::DbErr) -> Self {
        // SQL errors may include bound input or credentials. Never log/display their text.
        tracing::error!(error_kind = "database", "database operation failed");
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Database operation unavailable",
        )
    }
}
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // request_context renders the problem once its correlation ID is available.
        (self.status, Extension(self)).into_response()
    }
}

pub(crate) fn normalize(mut response: Response, request_id: &str) -> Response {
    let status = response.status();
    if !status.is_client_error() && !status.is_server_error() {
        return response;
    }

    // Only AppError supplies safe public detail; never inspect arbitrary response bodies.
    let error = response
        .extensions_mut()
        .remove::<AppError>()
        .unwrap_or_else(|| AppError::new(status, "Request failed"));
    let problem = Problem {
        kind: "about:blank".into(),
        title: status.canonical_reason().unwrap_or("Error").into(),
        status: status.as_u16(),
        detail: error.detail.into(),
        request_id: request_id.into(),
        issues: error.issues,
    };
    *response.body_mut() = Json(problem).into_response().into_body();
    let headers = response.headers_mut();
    for name in [
        "content-length",
        "content-encoding",
        "content-range",
        "etag",
        "last-modified",
    ] {
        headers.remove(name);
    }
    headers.insert(
        "content-type",
        axum::http::HeaderValue::from_static("application/problem+json"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use serde_json::{Value, json};

    #[tokio::test]
    async fn validation_preserves_issue_order_and_typed_paths() {
        let mut errors = ValidationErrors::default();
        errors.add(
            ["items".into(), 0usize.into(), "name".into()],
            IssueCode::TooSmall,
            "Name is required.",
        );
        let mut more = ValidationErrors::default();
        more.add(
            ["items".into(), 0usize.into(), "name".into()],
            IssueCode::Custom,
            "Name is reserved.",
        );
        more.add(
            ["literal.dot".into(), "0".into()],
            IssueCode::InvalidType,
            "Expected a string.",
        );
        errors.merge(more);
        let error = errors.finish().expect_err("validation must fail");
        let response = normalize(error.into_response(), "test-request");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let problem: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            problem["issues"],
            json!([
                {"code":"too_small", "path":["items",0,"name"], "message":"Name is required."},
                {"code":"custom", "path":["items",0,"name"], "message":"Name is reserved."},
                {"code":"invalid_type", "path":["literal.dot","0"], "message":"Expected a string."}
            ])
        );
        assert!(problem.get("errors").is_none());
        assert!(ValidationErrors::default().finish().is_ok());
    }
}
