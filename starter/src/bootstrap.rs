//! Application-owned provider construction and process lifecycle.
use crate::{AppState, config::Config};

#[derive(Clone, Default)]
pub struct Resources {
    #[cfg(feature = "batteries")]
    pub(crate) files: Option<bracel_files::Files>,
    #[cfg(feature = "batteries")]
    pub(crate) events: Option<bracel_realtime::EventStore>,
}
impl Resources {
    pub fn build(_state: &AppState, _config: &Config) -> Result<Self, String> {
        #[allow(unused_mut)]
        let mut resources = Self::default();
        #[cfg(feature = "batteries")]
        if _config.enable_batteries {
            resources.events = Some(
                bracel_realtime::EventStore::new(_state.db.clone(), 100)
                    .map_err(|_| "Invalid event configuration")?,
            );
            if let Some(root) = &_config.providers.files_root {
                std::fs::create_dir_all(root)
                    .map_err(|_| "Cannot create configured files directory")?;
                let storage = bracel_integrations::storage::Storage::local(root, 1024 * 1024)
                    .map_err(|_| "Invalid files storage")?;
                resources.files = Some(
                    bracel_files::Files::new(_state.db.clone(), storage, 1024 * 1024)
                        .map_err(|_| "Invalid files configuration")?,
                );
            }
        }
        Ok(resources)
    }
    pub fn shutdown(&self) {
        #[cfg(feature = "batteries")]
        if let Some(events) = &self.events {
            events.shutdown();
        }
    }
}

/// One signal owner broadcasts to the server and background tasks.
pub struct Lifecycle {
    stop: tokio::sync::watch::Sender<bool>,
    tasks: tokio::task::JoinSet<()>,
}
impl Default for Lifecycle {
    fn default() -> Self {
        let (stop, _) = tokio::sync::watch::channel(false);
        Self {
            stop,
            tasks: tokio::task::JoinSet::new(),
        }
    }
}
impl Lifecycle {
    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<bool> {
        self.stop.subscribe()
    }
    pub fn spawn(&mut self, task: impl std::future::Future<Output = ()> + Send + 'static) {
        self.tasks.spawn(task);
    }
    pub fn listen(&mut self) {
        let stop = self.stop.clone();
        let receiver = self.subscribe();
        self.spawn(async move { tokio::select! { _ = signal() => { stop.send_replace(true); }, _ = stopped(receiver) => {} } });
    }
    pub async fn finish(mut self) {
        self.stop.send_replace(true);
        let drain = async { while self.tasks.join_next().await.is_some() {} };
        if tokio::time::timeout(std::time::Duration::from_secs(5), drain)
            .await
            .is_err()
        {
            self.tasks.abort_all();
            while self.tasks.join_next().await.is_some() {}
        }
    }
}
pub async fn stopped(mut stop: tokio::sync::watch::Receiver<bool>) {
    while !*stop.borrow_and_update() {
        if stop.changed().await.is_err() {
            break;
        }
    }
}
pub async fn signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
