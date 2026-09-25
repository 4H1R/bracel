//! Strict, bounded URL decoding shared by collection endpoints.
use super::error::AppError;
use std::collections::BTreeMap;

pub fn decode(raw: &str) -> Result<BTreeMap<String, String>, AppError> {
    let invalid = || {
        AppError::new(
            axum::http::StatusCode::BAD_REQUEST,
            "Invalid collection query",
        )
    };
    if raw.len() > 4096 {
        return Err(invalid());
    }
    let bytes = raw.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(invalid());
        }
    }
    let mut fields = BTreeMap::new();
    for (key, value) in url::form_urlencoded::parse(bytes) {
        if fields.len() == 16
            || key.contains('\u{fffd}')
            || value.contains('\u{fffd}')
            || fields
                .insert(key.into_owned(), value.into_owned())
                .is_some()
        {
            return Err(invalid());
        }
    }
    Ok(fields)
}
