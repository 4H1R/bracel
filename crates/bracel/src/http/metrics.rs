use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Default)]
pub struct Metrics(Arc<Mutex<Measurements>>);
type Measurements = BTreeMap<(String, String, u16), Measurement>;
#[derive(Clone, Default, Serialize)]
pub struct Measurement {
    pub requests: u64,
    pub duration_micros: u64,
    pub maximum_micros: u64,
}
#[derive(Serialize)]
pub struct RouteMetric {
    pub method: String,
    pub route: String,
    pub status: u16,
    #[serde(flatten)]
    pub measurement: Measurement,
}
impl Metrics {
    pub fn observe(&self, method: &str, route: &str, status: u16, duration: Duration) {
        let Ok(mut metrics) = self.0.lock() else {
            return;
        };
        let method = if matches!(
            method,
            "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE" | "OPTIONS"
        ) {
            method
        } else {
            "OTHER"
        };
        let key = (method.to_owned(), route.to_owned(), status);
        if metrics.len() >= 512 && !metrics.contains_key(&key) {
            return;
        }
        let entry = metrics.entry(key).or_default();
        let micros = duration.as_micros().min(u64::MAX as u128) as u64;
        entry.requests = entry.requests.saturating_add(1);
        entry.duration_micros = entry.duration_micros.saturating_add(micros);
        entry.maximum_micros = entry.maximum_micros.max(micros);
    }
    pub fn snapshot(&self) -> Vec<RouteMetric> {
        self.0
            .lock()
            .map(|metrics| {
                metrics
                    .iter()
                    .map(|((method, route, status), measurement)| RouteMetric {
                        method: method.clone(),
                        route: route.clone(),
                        status: *status,
                        measurement: measurement.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
