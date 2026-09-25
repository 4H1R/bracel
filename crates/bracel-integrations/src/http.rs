use crate::Error;
use reqwest::{Client, Method, Url};
use std::{collections::BTreeSet, time::Duration};

#[derive(Clone)]
pub struct Outbound {
    client: Client,
    origins: BTreeSet<String>,
    max_bytes: usize,
}
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}
impl Outbound {
    /// Allow configured HTTPS origins; explicit numeric loopback HTTP origins support local tests.
    /// This is for administrator-configured services, not arbitrary user-supplied URL fetching.
    pub fn new(origins: &[&str], max_bytes: usize, timeout: Duration) -> Result<Self, Error> {
        if origins.is_empty()
            || max_bytes == 0
            || max_bytes > 16 * 1024 * 1024
            || timeout.is_zero()
            || timeout > Duration::from_secs(60)
        {
            return Err(Error::Configuration);
        }
        let mut allowed = BTreeSet::new();
        for origin in origins {
            let url = Url::parse(origin).map_err(|_| Error::Configuration)?;
            if url.origin().ascii_serialization() != *origin
                || (url.scheme() != "https"
                    && !(url.scheme() == "http"
                        && url
                            .host_str()
                            .is_some_and(|h| matches!(h, "127.0.0.1" | "[::1]"))))
            {
                return Err(Error::Configuration);
            }
            allowed.insert(origin.to_string());
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .user_agent("Bracel")
            .build()
            .map_err(|_| Error::Configuration)?;
        Ok(Self {
            client,
            origins: allowed,
            max_bytes,
        })
    }
    /// No automatic write retries. The caller decides retry and idempotency semantics.
    pub async fn request(
        &self,
        method: Method,
        url: &str,
        body: Option<Vec<u8>>,
    ) -> Result<Response, Error> {
        let url = Url::parse(url).map_err(|_| Error::InvalidInput)?;
        if !self.origins.contains(&url.origin().ascii_serialization())
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(Error::InvalidInput);
        }
        if body.as_ref().is_some_and(|b| b.len() > self.max_bytes) {
            return Err(Error::TooLarge);
        }
        let mut request = self.client.request(method, url);
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(body);
        }
        let mut response = request.send().await.map_err(|_| Error::Unavailable)?;
        if response
            .content_length()
            .is_some_and(|n| n > self.max_bytes as u64)
        {
            return Err(Error::TooLarge);
        }
        let status = response.status().as_u16();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Unavailable)? {
            if chunk.len() > self.max_bytes - body.len() {
                return Err(Error::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Response { status, body })
    }
}
pub use reqwest::Method as HttpMethod;
