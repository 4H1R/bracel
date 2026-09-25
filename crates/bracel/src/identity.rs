//! Issuer-signed access tokens. This module never issues tokens or accepts cookies.
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;

#[derive(Clone)]
pub struct BearerAuth {
    key: DecodingKey,
    validation: Validation,
}

#[derive(Clone, Deserialize)]
pub struct Principal {
    pub sub: String,
    pub iss: String,
    #[serde(default)]
    scope: String,
}

impl Principal {
    pub fn allows(&self, scope: &str) -> bool {
        self.scope.split_ascii_whitespace().any(|s| s == scope)
    }
    pub fn cursor_scope(&self) -> String {
        serde_json::to_string(&(&self.iss, &self.sub)).expect("principal scope")
    }
}

impl BearerAuth {
    pub fn new(pem: &str, issuer: &str, audience: &str) -> Result<Self, String> {
        if issuer.is_empty()
            || audience.is_empty()
            || issuer.len() > 512
            || audience.len() > 512
            || pem.len() > 16384
        {
            return Err("AUTH configuration is invalid".into());
        }
        let key = DecodingKey::from_rsa_pem(pem.as_bytes())
            .map_err(|_| "AUTH_PUBLIC_KEY_PEM must be an RSA public key")?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[issuer]);
        validation.set_audience(&[audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.validate_nbf = true;
        validation.leeway = 0;
        Ok(Self { key, validation })
    }

    pub fn verify(&self, token: &str) -> Option<Principal> {
        if token.len() > 8192 {
            return None;
        }
        let header = decode_header(token).ok()?;
        // Accept only access tokens from the configured key; never follow jku/x5u.
        if header.typ.as_deref() != Some("at+jwt") || header.crit.is_some() {
            return None;
        }
        let principal = decode::<Principal>(token, &self.key, &self.validation)
            .ok()?
            .claims;
        if principal.sub.is_empty() || principal.sub.len() > 200 || principal.scope.len() > 2048 {
            return None;
        }
        Some(principal)
    }
}
