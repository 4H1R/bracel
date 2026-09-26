use crate::Error;
use futures_util::StreamExt;
use object_store::{ObjectStore, ObjectStoreExt, path::Path};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;
#[derive(Clone)]
pub struct Storage {
    store: Arc<dyn ObjectStore>,
    max_bytes: usize,
    signer: Option<Arc<dyn object_store::signer::Signer>>,
}
impl Storage {
    pub fn new(store: Arc<dyn ObjectStore>, max_bytes: usize) -> Result<Self, Error> {
        if max_bytes == 0 || max_bytes > 16 * 1024 * 1024 {
            return Err(Error::Configuration);
        }
        Ok(Self {
            store,
            max_bytes,
            signer: None,
        })
    }
    pub fn memory(max_bytes: usize) -> Result<Self, Error> {
        Self::new(Arc::new(object_store::memory::InMemory::new()), max_bytes)
    }
    pub fn local(root: impl AsRef<std::path::Path>, max_bytes: usize) -> Result<Self, Error> {
        Self::new(
            Arc::new(
                object_store::local::LocalFileSystem::new_with_prefix(root)
                    .map_err(|_| Error::Configuration)?,
            ),
            max_bytes,
        )
    }
    pub fn s3_from_env(bucket: &str, max_bytes: usize) -> Result<Self, Error> {
        let store = object_store::aws::AmazonS3Builder::from_env()
            .with_bucket_name(bucket)
            .with_client_options(
                object_store::ClientOptions::new()
                    .with_timeout(Duration::from_secs(10))
                    .with_connect_timeout(Duration::from_secs(2)),
            )
            .build()
            .map_err(|_| Error::Configuration)?;
        let store = Arc::new(store);
        let mut storage = Self::new(store.clone(), max_bytes)?;
        storage.signer = Some(store);
        Ok(storage)
    }
    fn path(scope: &str, id: Uuid) -> Path {
        Path::from(format!("{:x}/{id}", Sha256::digest(scope.as_bytes())))
    }
    /// Supply a verified owner/tenant scope on every operation, never a client filename.
    pub async fn put(&self, scope: &str, bytes: Vec<u8>) -> Result<Uuid, Error> {
        if scope.is_empty() {
            return Err(Error::InvalidInput);
        }
        if bytes.len() > self.max_bytes {
            return Err(Error::TooLarge);
        }
        let id = Uuid::now_v7();
        self.put_at(scope, id, bytes).await?;
        Ok(id)
    }
    pub async fn put_at(&self, scope: &str, id: Uuid, bytes: Vec<u8>) -> Result<(), Error> {
        if scope.is_empty() {
            return Err(Error::InvalidInput);
        }
        if bytes.len() > self.max_bytes {
            return Err(Error::TooLarge);
        }
        tokio::time::timeout(
            Duration::from_secs(10),
            self.store.put(&Self::path(scope, id), bytes.into()),
        )
        .await
        .map_err(|_| Error::Unavailable)?
        .map_err(|_| Error::Unavailable)?;
        Ok(())
    }
    pub async fn signed_url(&self, scope: &str, id: Uuid, upload: bool) -> Result<String, Error> {
        if scope.is_empty() {
            return Err(Error::InvalidInput);
        }
        let signer = self.signer.as_ref().ok_or(Error::Configuration)?;
        tokio::time::timeout(
            Duration::from_secs(5),
            signer.signed_url(
                if upload {
                    reqwest::Method::PUT
                } else {
                    reqwest::Method::GET
                },
                &Self::path(scope, id),
                Duration::from_secs(300),
            ),
        )
        .await
        .map_err(|_| Error::Unavailable)?
        .map(|url| url.to_string())
        .map_err(|_| Error::Unavailable)
    }
    pub async fn get(&self, scope: &str, id: Uuid) -> Result<Vec<u8>, Error> {
        if scope.is_empty() {
            return Err(Error::InvalidInput);
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            let object =
                self.store
                    .get(&Self::path(scope, id))
                    .await
                    .map_err(|error| match error {
                        object_store::Error::NotFound { .. } => Error::NotFound,
                        _ => Error::Unavailable,
                    })?;
            if object.meta.size > self.max_bytes as u64 {
                return Err(Error::TooLarge);
            }
            let mut stream = object.into_stream();
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| Error::Unavailable)?;
                if chunk.len() > self.max_bytes - bytes.len() {
                    return Err(Error::TooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        })
        .await
        .map_err(|_| Error::Unavailable)?
    }
    pub async fn delete(&self, scope: &str, id: Uuid) -> Result<(), Error> {
        if scope.is_empty() {
            return Err(Error::InvalidInput);
        }
        tokio::time::timeout(
            Duration::from_secs(10),
            self.store.delete(&Self::path(scope, id)),
        )
        .await
        .map_err(|_| Error::Unavailable)?
        .or_else(|error| match error {
            object_store::Error::NotFound { .. } => Ok(()),
            _ => Err(Error::Unavailable),
        })
    }
}
