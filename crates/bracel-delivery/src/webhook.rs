use bracel::{axum::http::StatusCode, http::error::AppError};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub fn sign(secret: &[u8], timestamp: i64, id: &str, body: &[u8]) -> Result<String, AppError> {
    if secret.len() < 32
        || id.is_empty()
        || id.len() > 200
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        || body.len() > 65536
    {
        return Err(invalid());
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).map_err(|_| invalid())?;
    mac.update(format!("{timestamp}.{id}.").as_bytes());
    mac.update(body);
    Ok(mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub fn verify(
    secret: &[u8],
    timestamp: i64,
    now: i64,
    id: &str,
    body: &[u8],
    signature: &str,
) -> Result<(), AppError> {
    if timestamp.abs_diff(now) > 300
        || signature.len() != 64
        || !signature.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err(invalid());
    }
    sign(secret, timestamp, id, body)?;
    let bytes = (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&signature[i..i + 2], 16).map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).map_err(|_| invalid())?;
    mac.update(format!("{timestamp}.{id}.").as_bytes());
    mac.update(body);
    mac.verify_slice(&bytes).map_err(|_| invalid())
}
fn invalid() -> AppError {
    AppError::new(
        StatusCode::UNAUTHORIZED,
        "Webhook signature is invalid or expired",
    )
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct WebhookJob {
    pub destination: String,
    pub event_id: uuid::Uuid,
    pub payload: serde_json::Value,
}
impl bracel::jobs::TypedJob for WebhookJob {
    const KIND: &'static str = "delivery.webhook";
}
pub struct Endpoint {
    pub url: String,
    pub secret: Vec<u8>,
}
pub async fn enqueue(
    tx: &bracel::sea_orm::DatabaseTransaction,
    destination: &str,
    event_id: uuid::Uuid,
    payload: serde_json::Value,
) -> Result<uuid::Uuid, AppError> {
    if destination.is_empty()
        || destination.len() > 100
        || !destination
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(bracel_data::conflict());
    }
    Ok(bracel::jobs::dispatch(
        tx,
        &WebhookJob {
            destination: destination.into(),
            event_id,
            payload,
        },
        &format!("webhook:{destination}:{event_id}"),
        &bracel::jobs::DispatchOptions {
            queue: "webhooks".into(),
            ..Default::default()
        },
    )
    .await?)
}
pub fn register(
    worker: &mut bracel::jobs::Worker,
    endpoints: std::collections::BTreeMap<String, Endpoint>,
    client: bracel_integrations::http::Outbound,
) -> Result<(), &'static str> {
    use bracel::jobs::Failure;
    let endpoints = std::sync::Arc::new(endpoints);
    worker.register_typed(move |job: WebhookJob| {
        let (endpoints, client) = (endpoints.clone(), client.clone());
        async move {
            let endpoint = endpoints
                .get(&job.destination)
                .ok_or(Failure::Permanent("unknown_destination"))?;
            let body = serde_json::to_vec(&job.payload)
                .map_err(|_| Failure::Permanent("invalid_payload"))?;
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| Failure::Retry("clock"))?
                .as_secs() as i64;
            let id = job.event_id.to_string();
            let signature = sign(&endpoint.secret, timestamp, &id, &body)
                .map_err(|_| Failure::Permanent("invalid_signing_configuration"))?;
            let timestamp = timestamp.to_string();
            let response = client
                .request_with_headers(
                    bracel_integrations::http::HttpMethod::POST,
                    &endpoint.url,
                    Some(body),
                    &[
                        ("webhook-id", &id),
                        ("webhook-timestamp", &timestamp),
                        ("webhook-signature", &signature),
                    ],
                )
                .await
                .map_err(|_| Failure::Retry("webhook_unavailable"))?;
            match response.status {
                200..=299 => Ok(()),
                408 | 429 | 500..=599 => Err(Failure::Retry("webhook_retry")),
                _ => Err(Failure::Permanent("webhook_rejected")),
            }
        }
    })
}

pub struct SignedRequest<'a> {
    pub timestamp: i64,
    pub event_id: &'a str,
    pub body: &'a [u8],
    pub signature: &'a str,
}
pub async fn receive(
    db: &bracel::sea_orm::DatabaseConnection,
    provider: &str,
    secret: &[u8],
    request: SignedRequest<'_>,
) -> Result<bool, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| invalid())?
        .as_secs() as i64;
    verify(
        secret,
        request.timestamp,
        now,
        request.event_id,
        request.body,
        request.signature,
    )?;
    let payload = serde_json::from_slice(request.body)
        .map_err(|_| AppError::new(StatusCode::BAD_REQUEST, "Invalid webhook JSON"))?;
    super::receive_webhook(db, provider, request.event_id, payload).await
}
