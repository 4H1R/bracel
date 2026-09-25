//! Bounded local quotas. Keys are never emitted in responses or logs.
use governor::{
    DefaultDirectRateLimiter, Quota, RateLimiter,
    clock::{Clock, DefaultClock},
};
use std::{
    collections::HashMap,
    num::NonZeroU32,
    sync::Mutex,
    time::{Duration, Instant},
};

struct Entry {
    limiter: DefaultDirectRateLimiter,
    touched: Instant,
}
struct Store {
    entries: HashMap<String, Entry>,
    swept: Instant,
}
pub struct Limiter {
    store: Mutex<Store>,
    quota: Quota,
    capacity: usize,
}

pub enum Denied {
    Quota(u64),
    Capacity,
}

impl Limiter {
    pub fn new(per_minute: u32, capacity: usize) -> Self {
        Self {
            store: Mutex::new(Store {
                entries: HashMap::new(),
                swept: Instant::now(),
            }),
            quota: Quota::per_minute(NonZeroU32::new(per_minute).expect("validated quota")),
            capacity,
        }
    }
    pub fn check(&self, key: String) -> Result<(), Denied> {
        let now = Instant::now();
        let mut store = self.store.lock().map_err(|_| Denied::Capacity)?;
        // A minute of inactivity fully replenishes these per-minute buckets.
        if now.duration_since(store.swept) >= Duration::from_secs(60) {
            store
                .entries
                .retain(|_, entry| now.duration_since(entry.touched) < Duration::from_secs(60));
            store.swept = now;
        }
        if !store.entries.contains_key(&key) && store.entries.len() >= self.capacity {
            return Err(Denied::Capacity);
        }
        let entry = store.entries.entry(key).or_insert_with(|| Entry {
            limiter: RateLimiter::direct(self.quota),
            touched: now,
        });
        entry.touched = now;
        entry.limiter.check().map_err(|negative| {
            let wait = negative.wait_time_from(DefaultClock::default().now());
            Denied::Quota(
                wait.as_secs()
                    .saturating_add(u64::from(wait.subsec_nanos() != 0))
                    .max(1),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quota_and_capacity_are_separate_and_idle_keys_expire() {
        let limiter = Limiter::new(1, 1);
        assert!(limiter.check("one".into()).is_ok());
        assert!(matches!(
            limiter.check("one".into()),
            Err(Denied::Quota(1..=60))
        ));
        assert!(matches!(limiter.check("two".into()), Err(Denied::Capacity)));
        let past = Instant::now() - Duration::from_secs(61);
        {
            let mut store = limiter.store.lock().unwrap();
            store.swept = past;
            store.entries.get_mut("one").unwrap().touched = past;
        }
        assert!(limiter.check("two".into()).is_ok());
    }
}
