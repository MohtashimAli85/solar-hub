use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(60);
const NODE_CANDIDATES: &[&str] = &["node", "/opt/homebrew/bin/node", "/usr/local/bin/node"];

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("node was not found on PATH or in the usual homebrew locations")]
    NodeNotFound,
    #[error("agent script failed: {0}")]
    ScriptFailed(String),
    #[error("agent script timed out")]
    Timeout,
    #[error("agent io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("agent output was not valid: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SunToday {
    pub sunrise: String,
    pub sunset: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SunTomorrow {
    pub sunrise: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SunInfo {
    pub today: SunToday,
    pub tomorrow: SunTomorrow,
    pub altitude_deg: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Decision {
    pub mode: String,
    #[allow(dead_code)]
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Verification {
    pub verified: bool,
    #[allow(dead_code)]
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SunInput {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecideInput {
    pub soc: Option<f64>,
    pub discharge_a: f64,
    pub usable_capacity_ah: f64,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub mode: Option<String>,
    pub hour: u32,
    pub required_hours: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyInput {
    pub original_reason: String,
    pub soc: Option<f64>,
    pub usable_capacity_ah: f64,
    pub verified_discharge_a: f64,
    pub required_hours: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MorningInput {
    pub soc: Option<f64>,
    pub charge_a: Option<f64>,
    pub charge_threshold_a: f64,
    pub pv_w: Option<f64>,
    pub expected_pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub mode: Option<String>,
    pub minutes_since_sunrise: f64,
}

pub trait AgentRunner: Send + Sync {
    fn run_sun(&self, input: SunInput) -> BoxFuture<'_, Result<SunInfo, AgentError>>;
    fn run_decide(&self, input: DecideInput) -> BoxFuture<'_, Result<Decision, AgentError>>;
    fn run_verify(&self, input: VerifyInput) -> BoxFuture<'_, Result<Verification, AgentError>>;
    fn run_morning(&self, input: MorningInput) -> BoxFuture<'_, Result<Decision, AgentError>>;
}

pub struct NodeAgent;

impl NodeAgent {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NodeAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRunner for NodeAgent {
    fn run_sun(&self, input: SunInput) -> BoxFuture<'_, Result<SunInfo, AgentError>> {
        Box::pin(async move { run_script("sun.js", &input, None).await })
    }

    fn run_decide(&self, input: DecideInput) -> BoxFuture<'_, Result<Decision, AgentError>> {
        Box::pin(async move {
            let key = gemini_key()?;
            run_script("decide.js", &input, Some(&key)).await
        })
    }

    fn run_verify(&self, input: VerifyInput) -> BoxFuture<'_, Result<Verification, AgentError>> {
        Box::pin(async move {
            let key = gemini_key()?;
            run_script("verify.js", &input, Some(&key)).await
        })
    }

    fn run_morning(&self, input: MorningInput) -> BoxFuture<'_, Result<Decision, AgentError>> {
        Box::pin(async move {
            let key = gemini_key()?;
            run_script("morning.js", &input, Some(&key)).await
        })
    }
}

fn gemini_key() -> Result<String, AgentError> {
    crate::storage::get_gemini_api_key()
        .map_err(AgentError::ScriptFailed)?
        .ok_or_else(|| AgentError::ScriptFailed("GEMINI_API_KEY is not set".into()))
}

fn agent_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../agent"))
}

async fn run_script<I, O>(script: &str, input: &I, gemini_key: Option<&str>) -> Result<O, AgentError>
where
    I: Serialize,
    O: DeserializeOwned,
{
    let payload = serde_json::to_vec(input)?;
    let mut last_error = AgentError::NodeNotFound;
    for _ in 0..2 {
        match run_once(script, &payload, gemini_key).await {
            Ok(bytes) => return Ok(serde_json::from_slice(&bytes)?),
            Err(error) => {
                tracing::debug!("{script} attempt failed: {error}");
                last_error = error;
            }
        }
    }
    Err(last_error)
}

async fn run_once(script: &str, payload: &[u8], gemini_key: Option<&str>) -> Result<Vec<u8>, AgentError> {
    let mut last_error = AgentError::NodeNotFound;
    for node in NODE_CANDIDATES {
        let mut command = Command::new(node);
        command
            .current_dir(agent_dir())
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(key) = gemini_key {
            command.env("GEMINI_API_KEY", key);
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                last_error = AgentError::Io(error);
                continue;
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(payload).await;
        }

        let output = match tokio::time::timeout(TIMEOUT, child.wait_with_output()).await {
            Ok(result) => result?,
            Err(_) => return Err(AgentError::Timeout),
        };
        if let Ok(stderr) = String::from_utf8(output.stderr) {
            if !stderr.trim().is_empty() {
                tracing::debug!("{script} stderr: {stderr}");
            }
        }
        if !output.status.success() {
            return Err(AgentError::ScriptFailed(format!(
                "{script} exited with {:?}",
                output.status.code()
            )));
        }
        return Ok(output.stdout);
    }
    Err(last_error)
}
