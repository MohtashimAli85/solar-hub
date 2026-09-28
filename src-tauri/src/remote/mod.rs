pub mod commands;
mod routes;

use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tokio::sync::{watch, Mutex};

use crate::storage::save_remote_config;

pub const DEFAULT_PORT: u16 = 8787;
const MAX_PIN_ATTEMPTS: u32 = 5;
const LOCKOUT: Duration = Duration::from_secs(60);
const STOP_TIMEOUT: Duration = Duration::from_secs(3);

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn new_pin() -> String {
    format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "new_pin")]
    pub pin: String,
    #[serde(default)]
    pub token_hashes: Vec<String>,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: DEFAULT_PORT,
            pin: new_pin(),
            token_hashes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub pin: String,
    pub urls: Vec<String>,
    pub paired_devices: usize,
    pub error: Option<String>,
}

pub enum PairError {
    WrongPin,
    Locked(u64),
    Save(String),
}

struct Server {
    shutdown: watch::Sender<bool>,
    task: tauri::async_runtime::JoinHandle<()>,
}

struct Inner {
    config: RemoteConfig,
    server: Option<Server>,
    error: Option<String>,
    failed_attempts: u32,
    locked_until: Option<Instant>,
}

#[derive(Clone)]
pub struct RemoteState {
    inner: Arc<Mutex<Inner>>,
}

impl RemoteState {
    pub fn new(config: RemoteConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                config,
                server: None,
                error: None,
                failed_attempts: 0,
                locked_until: None,
            })),
        }
    }

    pub async fn enabled(&self) -> bool {
        self.inner.lock().await.config.enabled
    }

    pub async fn status(&self) -> RemoteStatus {
        let inner = self.inner.lock().await;
        let running = inner.server.is_some();
        let urls = if running {
            local_ip_address::local_ip()
                .map(|ip| vec![format!("http://{ip}:{}", inner.config.port)])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        RemoteStatus {
            enabled: inner.config.enabled,
            running,
            port: inner.config.port,
            pin: inner.config.pin.clone(),
            urls,
            paired_devices: inner.config.token_hashes.len(),
            error: inner.error.clone(),
        }
    }

    pub async fn start(&self, app: &AppHandle) {
        self.stop().await;
        let port = self.inner.lock().await.config.port;
        let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!("phone access could not listen on port {port}: {error}");
                self.inner.lock().await.error = Some(format!("Could not open port {port}: {error}"));
                return;
            }
        };
        let (shutdown, shutdown_rx) = watch::channel(false);
        let router = routes::router(routes::Ctx {
            app: app.clone(),
            remote: self.clone(),
            shutdown: shutdown_rx.clone(),
        });
        let mut stop_signal = shutdown_rx;
        let task = tauri::async_runtime::spawn(async move {
            let result = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = stop_signal.wait_for(|stop| *stop).await;
                })
                .await;
            if let Err(error) = result {
                tracing::warn!("phone access server stopped: {error}");
            }
        });
        tracing::info!("phone access listening on port {port}");
        let mut inner = self.inner.lock().await;
        inner.error = None;
        inner.server = Some(Server { shutdown, task });
    }

    pub async fn stop(&self) {
        let server = self.inner.lock().await.server.take();
        if let Some(server) = server {
            let _ = server.shutdown.send(true);
            if tokio::time::timeout(STOP_TIMEOUT, server.task).await.is_err() {
                tracing::warn!("phone access server did not stop in time");
            }
        }
    }

    pub async fn set_enabled(&self, app: &AppHandle, enabled: bool) -> Result<RemoteStatus, String> {
        let config = {
            let mut inner = self.inner.lock().await;
            inner.config.enabled = enabled;
            inner.config.clone()
        };
        save_remote_config(app, &config)?;
        if enabled {
            self.start(app).await;
        } else {
            self.stop().await;
            self.inner.lock().await.error = None;
        }
        Ok(self.status().await)
    }

    pub async fn regenerate_pin(&self, app: &AppHandle) -> Result<RemoteStatus, String> {
        let (config, running) = {
            let mut inner = self.inner.lock().await;
            inner.config.pin = new_pin();
            inner.config.token_hashes.clear();
            inner.failed_attempts = 0;
            inner.locked_until = None;
            (inner.config.clone(), inner.server.is_some())
        };
        save_remote_config(app, &config)?;
        if running {
            self.start(app).await;
        }
        Ok(self.status().await)
    }

    pub async fn pair(&self, app: &AppHandle, pin: &str) -> Result<String, PairError> {
        let config = {
            let mut inner = self.inner.lock().await;
            let now = Instant::now();
            if let Some(until) = inner.locked_until {
                if until > now {
                    return Err(PairError::Locked((until - now).as_secs().max(1)));
                }
                inner.locked_until = None;
                inner.failed_attempts = 0;
            }
            if pin != inner.config.pin {
                inner.failed_attempts += 1;
                if inner.failed_attempts >= MAX_PIN_ATTEMPTS {
                    inner.locked_until = Some(now + LOCKOUT);
                }
                return Err(PairError::WrongPin);
            }
            inner.failed_attempts = 0;
            let token = hex::encode(rand::thread_rng().gen::<[u8; 32]>());
            inner.config.token_hashes.push(hash_token(&token));
            (inner.config.clone(), token)
        };
        save_remote_config(app, &config.0).map_err(PairError::Save)?;
        Ok(config.1)
    }

    pub async fn is_authorized(&self, token: Option<&str>) -> bool {
        let Some(token) = token.filter(|token| !token.is_empty()) else {
            return false;
        };
        let hash = hash_token(token);
        self.inner.lock().await.config.token_hashes.contains(&hash)
    }
}
