use crate::http::error::{self, AppError};
use axum::{
    extract::{MatchedPath, Request, State},
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::time::{Duration, Instant};
use tracing::Instrument;
#[derive(Clone)]
pub struct RequestContext {
    pub request_id: String,
}

pub(super) async fn deadline(
    State(timeout): State<Duration>,
    request: Request,
    next: Next,
) -> Response {
    match tokio::time::timeout(timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => {
            AppError::new(StatusCode::REQUEST_TIMEOUT, "Request deadline exceeded").into_response()
        }
    }
}
pub(super) async fn request_context(mut request: Request, next: Next) -> Response {
    let request_id = uuid::Uuid::now_v7().to_string();
    request.extensions_mut().insert(RequestContext {
        request_id: request_id.clone(),
    });
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str())
        .unwrap_or("unmatched")
        .to_owned();
    let metrics = request
        .extensions()
        .get::<crate::http::metrics::Metrics>()
        .cloned();
    let method = request.method().as_str().to_owned();
    // Never record raw URI/query, incoming request ID, headers, or request/response bodies.
    let span = tracing::info_span!("http_request", request_id = %request_id, route = %route);
    async {
        let started = Instant::now();
        let response = next.run(request).await;
        let mut response = error::normalize(response, &request_id);
        if let Some(metrics) = metrics {
            metrics.observe(
                &method,
                &route,
                response.status().as_u16(),
                started.elapsed(),
            );
        }
        response.headers_mut().insert(
            "x-request-id",
            HeaderValue::from_str(&request_id).expect("UUID is a valid header"),
        );
        response.headers_mut().insert(
            "x-content-type-options",
            HeaderValue::from_static("nosniff"),
        );
        tracing::info!(
            status = response.status().as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "request completed"
        );
        response
    }
    .instrument(span)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::{Json, Router, middleware, routing::get};
    use serde_json::{Value, json};
    use tower::ServiceExt;

    #[tokio::test]
    async fn errors_preserve_headers_and_only_expose_explicit_public_details() {
        let router = Router::new()
            .route(
                "/known",
                get(|| async {
                    (
                        [
                            ("retry-after", "30"),
                            ("www-authenticate", "Bearer"),
                            ("access-control-allow-origin", "https://client.example"),
                            ("set-cookie", "session=; Max-Age=0"),
                        ],
                        AppError::new(StatusCode::TOO_MANY_REQUESTS, "Try later"),
                    )
                }),
            )
            .route(
                "/unknown",
                get(|| async {
                    (
                        StatusCode::BAD_GATEWAY,
                        [
                            ("content-length", "999"),
                            ("content-encoding", "gzip"),
                            ("etag", "old-body"),
                        ],
                        Json(json!({"detail": "provider-secret"})),
                    )
                }),
            )
            .route("/success", get(|| async { "unchanged" }))
            .layer(middleware::from_fn(request_context));

        for (path, expected_status, expected_detail) in [
            ("/known", StatusCode::TOO_MANY_REQUESTS, "Try later"),
            ("/unknown", StatusCode::BAD_GATEWAY, "Request failed"),
        ] {
            let response = router
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected_status);
            let headers = response.headers().clone();
            assert_eq!(headers["content-type"], "application/problem+json");
            assert!(!headers.contains_key("content-encoding"));
            assert!(!headers.contains_key("etag"));
            let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
            if let Some(length) = headers.get("content-length") {
                assert_eq!(
                    length.to_str().unwrap().parse::<usize>().unwrap(),
                    bytes.len()
                );
            }
            let problem: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(problem["detail"], expected_detail);
            assert_eq!(
                problem["request_id"],
                headers["x-request-id"].to_str().unwrap()
            );
            assert!(!problem.to_string().contains("provider-secret"));
            if path == "/known" {
                assert_eq!(headers["retry-after"], "30");
                assert_eq!(headers["www-authenticate"], "Bearer");
                assert_eq!(
                    headers["access-control-allow-origin"],
                    "https://client.example"
                );
                assert_eq!(headers["set-cookie"], "session=; Max-Age=0");
            }
        }
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/success")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), 4096).await.unwrap(),
            "unchanged"
        );
    }
}
