use crate::{
    Error,
    http::{HttpMethod, Outbound},
};
use bracel::identity::BearerAuth;
use std::time::Duration;

pub struct IdentityProvider {
    client: Outbound,
    jwks: String,
    auth: BearerAuth,
}
impl IdentityProvider {
    /// Issuer is administrator-configured. Discovery cannot change its origin or verifier claims.
    pub async fn connect(issuer: &str, auth: BearerAuth) -> Result<Self, Error> {
        let url = reqwest::Url::parse(issuer).map_err(|_| Error::Configuration)?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::Configuration);
        }
        let origin = url.origin().ascii_serialization();
        let client = Outbound::new(&[&origin], 65536, Duration::from_secs(5))?;
        let metadata = client
            .request(
                HttpMethod::GET,
                &format!(
                    "{}/.well-known/openid-configuration",
                    issuer.trim_end_matches('/')
                ),
                None,
            )
            .await?;
        if metadata.status != 200 {
            return Err(Error::Unavailable);
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(&metadata.body).map_err(|_| Error::Rejected)?;
        if metadata["issuer"] != issuer {
            return Err(Error::Rejected);
        }
        let jwks = metadata["jwks_uri"]
            .as_str()
            .ok_or(Error::Rejected)?
            .to_owned();
        if reqwest::Url::parse(&jwks)
            .map_err(|_| Error::Rejected)?
            .origin()
            .ascii_serialization()
            != origin
        {
            return Err(Error::Rejected);
        }
        let provider = Self { client, jwks, auth };
        provider.refresh().await?;
        Ok(provider)
    }
    pub async fn refresh(&self) -> Result<(), Error> {
        let response = self
            .client
            .request(HttpMethod::GET, &self.jwks, None)
            .await?;
        if response.status != 200 {
            return Err(Error::Unavailable);
        }
        let document = std::str::from_utf8(&response.body).map_err(|_| Error::Rejected)?;
        self.auth
            .replace_jwks(document, Duration::from_secs(300))
            .map_err(|_| Error::Rejected)
    }
    pub async fn run(self, mut stop: tokio::sync::watch::Receiver<bool>) {
        while !*stop.borrow() {
            tokio::select! {_=tokio::time::sleep(Duration::from_secs(30))=>{let _=self.refresh().await;},_=stop.changed()=>break}
        }
    }
}
