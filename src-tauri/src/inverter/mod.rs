pub mod client;
pub mod commands;
pub mod crypto;
pub mod types;

use std::sync::Arc;

use client::SolarClient;

#[derive(Clone)]
pub struct InverterState {
    client: Arc<SolarClient>,
}

impl InverterState {
    pub fn new(client: SolarClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    pub fn solar_client(&self) -> SolarClient {
        self.client.as_ref().clone()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InverterError {
    #[error("credentials not configured: {0}")]
    Config(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("solar API error: {0}")]
    Api(String),
    #[error("solar auth error: {0}")]
    Auth(String),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid time zone: {0}")]
    TimeZone(String),
}