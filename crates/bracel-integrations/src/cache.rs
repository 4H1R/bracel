use crate::Error;
use moka::future::Cache;
use sha2::{Digest, Sha256};
use std::{future::Future, sync::Arc, time::Duration};
#[derive(Clone)]
pub struct ScopedCache {
    cache: Cache<String, Vec<u8>>,
    max_item: usize,
}
impl ScopedCache {
    pub async fn get(&self, scope: &str, key: &str) -> Option<Vec<u8>> {
        self.cache.get(&Self::key(scope, key)).await
    }

    pub async fn put(&self, scope: &str, key: &str, value: Vec<u8>) -> Result<(), Error> {
        if value.len() > self.max_item {
            return Err(Error::TooLarge);
        }
        self.cache.insert(Self::key(scope, key), value).await;
        Ok(())
    }

    pub async fn forget(&self, scope: &str, key: &str) {
        self.invalidate(scope, key).await;
    }

    pub async fn remember<F>(&self, scope: &str, key: &str, load: F) -> Result<Vec<u8>, Arc<Error>>
    where
        F: Future<Output = Result<Vec<u8>, Error>>,
    {
        self.get_or_load(scope, key, load).await
    }
    pub fn new(capacity_bytes: u64, ttl: Duration, max_item_bytes: usize) -> Result<Self, Error> {
        if capacity_bytes == 0
            || max_item_bytes == 0
            || max_item_bytes as u64 > capacity_bytes
            || max_item_bytes > 16 * 1024 * 1024
            || ttl.is_zero()
        {
            return Err(Error::Configuration);
        }
        Ok(Self {
            cache: Cache::builder()
                .max_capacity(capacity_bytes)
                .time_to_live(ttl)
                .weigher(|key: &String, value: &Vec<u8>| (key.len() + value.len()) as u32)
                .build(),
            max_item: max_item_bytes,
        })
    }
    fn key(scope: &str, key: &str) -> String {
        format!(
            "{:x}",
            Sha256::digest(format!("{}:{scope}{key}", scope.len()).as_bytes())
        )
    }
    pub async fn get_or_load<F>(
        &self,
        scope: &str,
        key: &str,
        load: F,
    ) -> Result<Vec<u8>, Arc<Error>>
    where
        F: Future<Output = Result<Vec<u8>, Error>>,
    {
        self.cache
            .try_get_with(Self::key(scope, key), async {
                let value = load.await?;
                if value.len() > self.max_item {
                    return Err(Error::TooLarge);
                }
                Ok(value)
            })
            .await
    }
    pub async fn invalidate(&self, scope: &str, key: &str) {
        self.cache.invalidate(&Self::key(scope, key)).await;
    }
}
