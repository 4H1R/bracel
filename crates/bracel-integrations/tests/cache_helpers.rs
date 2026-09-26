#![cfg(feature = "cache")]
use bracel_integrations::{Error, cache::ScopedCache};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[tokio::test]
async fn cache_helpers_share_scopes_bounds_and_loader_semantics() {
    let cache = ScopedCache::new(4096, Duration::from_millis(80), 32).unwrap();
    cache
        .put("alice", "profile", b"cached".to_vec())
        .await
        .unwrap();
    assert_eq!(
        cache.get("alice", "profile").await,
        Some(b"cached".to_vec())
    );
    assert_eq!(cache.get("bob", "profile").await, None);
    assert_eq!(
        cache.put("alice", "large", vec![0; 33]).await,
        Err(Error::TooLarge)
    );
    cache.forget("alice", "profile").await;
    assert_eq!(cache.get("alice", "profile").await, None);
    assert!(
        cache
            .remember("alice", "profile", async { Err(Error::Unavailable) })
            .await
            .is_err()
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let loader = || {
        let calls = calls.clone();
        async move {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            Ok(b"loaded".to_vec())
        }
    };
    let (a, b) = tokio::join!(
        cache.remember("alice", "profile", loader()),
        cache.remember("alice", "profile", loader())
    );
    assert_eq!(a.unwrap(), b.unwrap());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(cache.get("alice", "profile").await, None);
}
