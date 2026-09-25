//! Issuer-signed access tokens. This module never issues tokens or accepts cookies.
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

#[derive(Clone)]
pub struct BearerAuth {
    key: Option<DecodingKey>,
    keys: Arc<RwLock<BTreeMap<String, DecodingKey>>>,
    #[cfg(feature = "tokens")]
    tokens: Option<sea_orm::DatabaseConnection>,
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
    pub fn new(issuer: String, subject: String, scope: String) -> Result<Self, &'static str> {
        if issuer.is_empty()
            || issuer.len() > 512
            || subject.is_empty()
            || subject.len() > 200
            || scope.len() > 2048
        {
            return Err("Invalid principal");
        }
        Ok(Self {
            iss: issuer,
            sub: subject,
            scope,
        })
    }
    pub fn scopes(&self) -> &str {
        &self.scope
    }

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
        Ok(Self {
            key: Some(key),
            keys: Arc::new(RwLock::new(BTreeMap::new())),
            validation,
            #[cfg(feature = "tokens")]
            tokens: None,
        })
    }

    /// Key-set mode requires a recognized kid; untrusted key URLs are never fetched.
    pub fn from_keys(
        keys: BTreeMap<String, String>,
        issuer: &str,
        audience: &str,
    ) -> Result<Self, String> {
        let pem = keys
            .values()
            .next()
            .ok_or("At least one RSA key is required")?;
        let mut auth = Self::new(pem, issuer, audience)?;
        auth.key = None;
        auth.replace_keys(keys)?;
        Ok(auth)
    }
    /// Atomically replace the complete set after validating every key. Clones share updates.
    pub fn replace_keys(&self, keys: BTreeMap<String, String>) -> Result<(), String> {
        if self.key.is_some() || keys.is_empty() || keys.len() > 16 {
            return Err("Key rotation requires a set of 1..16 keys".into());
        }
        let mut parsed = BTreeMap::new();
        for (kid, pem) in keys {
            if kid.is_empty() || kid.len() > 100 || pem.len() > 16384 {
                return Err("Invalid authentication key set".into());
            }
            parsed.insert(
                kid,
                DecodingKey::from_rsa_pem(pem.as_bytes())
                    .map_err(|_| "Invalid authentication key set")?,
            );
        }
        *self
            .keys
            .write()
            .map_err(|_| "Authentication keys unavailable")? = parsed;
        Ok(())
    }
    #[cfg(feature = "tokens")]
    pub fn with_tokens(mut self, db: sea_orm::DatabaseConnection) -> Self {
        self.tokens = Some(db);
        self
    }
    pub async fn verify_access(&self, token: &str) -> Result<Option<Principal>, &'static str> {
        #[cfg(feature = "tokens")]
        if token.starts_with("brc_") {
            return match &self.tokens {
                Some(db) => crate::tokens::verify(db, token)
                    .await
                    .map_err(|_| "Authentication unavailable"),
                None => Ok(None),
            };
        }
        Ok(self.verify(token))
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
        let keys = self.keys.read().ok()?;
        let key = if let Some(key) = &self.key {
            key
        } else {
            keys.get(header.kid.as_deref()?)?
        };
        let principal = decode::<Principal>(token, key, &self.validation)
            .ok()?
            .claims;
        if principal.sub.is_empty() || principal.sub.len() > 200 || principal.scope.len() > 2048 {
            return None;
        }
        Some(principal)
    }
}
