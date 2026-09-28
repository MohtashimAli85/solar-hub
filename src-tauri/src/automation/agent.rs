use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use futures_util::future::BoxFuture;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(60);
const RETRY_DELAY: Duration = Duration::from_secs(20);
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

impl AgentError {
    /// A short cause the homeowner can act on, for notifications.
    pub fn user_reason(&self) -> String {
        match self {
            AgentError::NodeNotFound => "Node.js wasn't found".into(),
            AgentError::ScriptFailed(summary) => summary.clone(),
            AgentError::Timeout => "the AI took too long to answer".into(),
            AgentError::Io(_) => "couldn't start the AI script".into(),
            AgentError::Json(_) => "the AI's answer wasn't valid".into(),
        }
    }
}

/// Turns a failed script's stderr into the likely cause.
fn classify_failure(stderr: &str) -> &'static str {
    let text = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if has(&["econnreset", "enotfound", "etimedout", "eai_again", "econnrefused", "fetch failed", "network", "socket"]) {
        "no internet connection"
    } else if has(&["api_key_invalid", "api key not valid", "permission_denied", "401", "403", "gemini_api_key is not set"]) {
        "Gemini rejected the API key"
    } else if has(&["429", "resource_exhausted", "quota", "rate limit"]) {
        "Gemini quota or rate limit reached"
    } else if has(&["503", "500", "unavailable", "overloaded", "internal"]) {
        "Gemini is busy or down"
    } else {
        "the AI script failed — see the log"
    }
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

#[derive(Debug, Clone, Serialize)]
pub struct SunInput {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviousPlan {
    pub reserve_soc: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HourLoadRow {
    pub hour: String,
    pub load_w: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SocRow {
    pub time: String,
    pub soc: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutageRow {
    pub when: String,
    pub minutes: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentDayRow {
    pub date: String,
    pub radiation_kwh_m2: Option<f64>,
    pub max_soc: Option<f64>,
    pub full_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NextDayRow {
    pub date: String,
    /// "today" before sunrise, "tomorrow" after it.
    pub relative: String,
    pub radiation_kwh_m2: Option<f64>,
    pub sunshine_h: Option<f64>,
    pub cloud_pct: Option<f64>,
    pub rain_prob_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeatherBrief {
    pub next_day: Option<NextDayRow>,
    pub recent_avg_radiation_kwh_m2: Option<f64>,
    pub recent_days: Vec<RecentDayRow>,
    pub tonight_min_temp_c: Option<f64>,
    pub recent_nights_min_temp_c: Option<f64>,
    pub expected_pv_kwh_next_day: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmartLoadBrief {
    pub season: String,
    pub currently_on: Option<bool>,
    pub choose_on_time: bool,
    pub earliest: String,
    pub latest: String,
}

/// Everything is pre-computed here so the model reasons instead of doing
/// arithmetic.
#[derive(Debug, Clone, Serialize)]
pub struct NightInput {
    pub now: String,
    /// The day whose sun will next charge the battery: "today" after
    /// midnight, "tomorrow" in the evening.
    pub coming_day: String,
    pub month: String,
    pub latitude: f64,
    pub on_battery: bool,
    pub on_battery_since: Option<String>,
    pub previous_plan: Option<PreviousPlan>,
    pub soc: f64,
    pub rated_capacity_ah: f64,
    pub battery_v: Option<f64>,
    pub floor_soc: f64,
    pub load_w: Option<f64>,
    pub pv_w: Option<f64>,
    pub discharge_a: f64,
    pub discharge_measured: bool,
    pub sunrise: String,
    pub hours_until_sunrise: f64,
    pub history_nights: u32,
    pub typical_load_now_w: Option<f64>,
    pub typical_hourly_load: Vec<HourLoadRow>,
    pub quiet_by: Option<String>,
    pub trajectory: Vec<SocRow>,
    pub trajectory_basis: String,
    pub soc_per_backup_hour: Option<f64>,
    pub outages: Vec<OutageRow>,
    pub weather: Option<WeatherBrief>,
    pub smart_load: SmartLoadBrief,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NightPlan {
    pub mode: String,
    pub reserve_soc: f64,
    pub recheck_minutes: f64,
    pub confidence: f64,
    pub reason: String,
    #[serde(default)]
    pub smart_load_on_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HourOutlookRow {
    pub time: String,
    pub cloud_pct: Option<f64>,
    pub radiation_w_m2: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayInput {
    pub now: String,
    pub soc: f64,
    pub rated_capacity_ah: f64,
    pub ah_to_full: f64,
    pub charge_a: f64,
    pub max_charge_a: Option<f64>,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub typical_load_now_w: Option<f64>,
    pub current_mode: String,
    pub battery_draining: bool,
    pub sunset: String,
    pub hours_of_sun_left: f64,
    pub projected_soc_at_sunset: f64,
    pub projected_full_at: Option<String>,
    pub projection_method: String,
    pub next_hours: Vec<HourOutlookRow>,
    pub recent_days: Vec<RecentDayRow>,
    pub forecast_available: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DayDecision {
    pub mode: String,
    pub recheck_minutes: f64,
    pub confidence: f64,
    pub reason: String,
}

pub trait AgentRunner: Send + Sync {
    fn run_sun(&self, input: SunInput) -> BoxFuture<'_, Result<SunInfo, AgentError>>;
    fn run_night(&self, input: NightInput) -> BoxFuture<'_, Result<NightPlan, AgentError>>;
    fn run_day(&self, input: DayInput) -> BoxFuture<'_, Result<DayDecision, AgentError>>;
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

    fn run_night(&self, input: NightInput) -> BoxFuture<'_, Result<NightPlan, AgentError>> {
        Box::pin(async move {
            let key = gemini_key()?;
            run_script("night.js", &input, Some(&key)).await
        })
    }

    fn run_day(&self, input: DayInput) -> BoxFuture<'_, Result<DayDecision, AgentError>> {
        Box::pin(async move {
            let key = gemini_key()?;
            run_script("day.js", &input, Some(&key)).await
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
    for attempt in 1..=2 {
        if attempt > 1 {
            tokio::time::sleep(RETRY_DELAY).await;
        }
        match run_once(script, &payload, gemini_key).await {
            Ok(bytes) => return Ok(serde_json::from_slice(&bytes)?),
            Err(error) => {
                tracing::warn!("{script} attempt {attempt}/2 failed: {error}");
                if matches!(error, AgentError::NodeNotFound) {
                    return Err(error);
                }
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
            .stderr(Stdio::piped())
            .kill_on_drop(true);
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
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !stderr.trim().is_empty() {
            tracing::debug!("{script} stderr: {stderr}");
        }
        if !output.status.success() {
            return Err(AgentError::ScriptFailed(classify_failure(&stderr).into()));
        }
        return Ok(output.stdout);
    }
    if matches!(last_error, AgentError::Io(ref error) if error.kind() == std::io::ErrorKind::NotFound) {
        return Err(AgentError::NodeNotFound);
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_are_named_by_their_cause() {
        let network = "TypeError: fetch failed\n  [cause]: Error: Client network socket disconnected before secure TLS connection was established {\n    code: 'ECONNRESET'";
        assert_eq!(classify_failure(network), "no internet connection");
        assert_eq!(classify_failure("ApiError: {\"error\":{\"code\":400,\"message\":\"API key not valid\"}}"), "Gemini rejected the API key");
        assert_eq!(classify_failure("got status: 429 RESOURCE_EXHAUSTED"), "Gemini quota or rate limit reached");
        assert_eq!(classify_failure("got status: 503 UNAVAILABLE. The model is overloaded"), "Gemini is busy or down");
        assert_eq!(classify_failure("SyntaxError: Unexpected token"), "the AI script failed — see the log");
    }
}
