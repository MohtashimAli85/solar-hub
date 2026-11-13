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
/// Names why the agent script failed. The script tries several models, so
/// stderr can hold mixed errors; an overloaded model wins over a dropped
/// connection, since the other models were reachable.
fn classify_failure(stderr: &str) -> &'static str {
    let text = stderr.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if has(&["unavailable", "overloaded", "high demand", "\"code\":503", "status: 503", "timeouterror", "operation was aborted"]) {
        "the AI models are busy or down"
    } else if has(&["econnreset", "enotfound", "etimedout", "eai_again", "econnrefused", "fetch failed", "network", "socket"]) {
        "no internet connection"
    } else if has(&["api_key_invalid", "api key not valid", "permission_denied", "401", "403", "api_key is not set", "api_key are not set"]) {
        "an AI provider rejected its API key"
    } else if has(&["429", "resource_exhausted", "quota", "rate limit"]) {
        "the AI quota or rate limit was reached"
    } else if has(&["\"code\":500", "status: 500", "internal error"]) {
        "the AI models are busy or down"
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
    /// The same load as battery current, if the house ran on battery.
    pub draw_a: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NightOutcomeRow {
    pub night: String,
    pub battery_from: Option<String>,
    pub soc_at_battery_start: Option<f64>,
    pub battery_until: Option<String>,
    pub lowest_soc: Option<f64>,
    pub morning_soc: Option<f64>,
    pub evening_draw_a: Option<f64>,
    pub sleep_draw_a: Option<f64>,
    pub next_day_full_at: Option<String>,
    pub next_day_max_soc: Option<f64>,
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
    /// Hours of it between midnight and the sun taking over, when the
    /// battery is usually down at its reserve.
    pub before_sun_hours: f64,
    /// The reserve that would have carried those hours at the sleeping load.
    pub reserve_to_cover: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentDayRow {
    pub date: String,
    pub radiation_kwh_m2: Option<f64>,
    pub cloud_pct: Option<f64>,
    pub rain_prob_pct: Option<f64>,
    pub morning_low_soc: Option<f64>,
    pub max_soc: Option<f64>,
    pub full_at: Option<String>,
}

/// What the recent days say about the sun refilling the battery.
#[derive(Debug, Clone, Serialize)]
pub struct RefillSummary {
    pub days: usize,
    pub full_days: usize,
    pub latest_full_at: Option<String>,
    /// The lowest morning SOC on a day that still got full.
    pub lowest_start_filled: Option<f64>,
    pub darkest_full: Option<RecentDayRow>,
    pub wettest_full: Option<RecentDayRow>,
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
    pub refill: Option<RefillSummary>,
    pub tonight_min_temp_c: Option<f64>,
    pub recent_nights_min_temp_c: Option<f64>,
    pub expected_pv_kwh_next_day: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SmartLoadBrief {
    pub currently_on: Option<bool>,
    pub trigger_amps: f64,
    pub trigger_minutes: u32,
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
    /// The floor plus what Solar mode's standby draw takes until sunrise;
    /// the controller never stops on battery below it.
    pub min_reserve_soc: f64,
    pub standby_drain_a: f64,
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
    /// The usual night start (HH:MM). Before it the battery may start early,
    /// but only when it can easily carry the evening and the night.
    pub usual_night_start: String,
    pub before_usual_start: bool,
    /// Before the usual start: the SOC left at sunrise if the battery waits for
    /// the usual start and then runs the rest of the night. Well above the
    /// reserve means waiting leaves battery unused.
    pub sunrise_soc_if_waiting: Option<f64>,
    pub recent_nights: Vec<NightOutcomeRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NightPlan {
    pub mode: String,
    pub reserve_soc: f64,
    pub recheck_minutes: f64,
    pub confidence: f64,
    pub reason: String,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HourOutlookRow {
    pub time: String,
    pub cloud_pct: Option<f64>,
    pub radiation_w_m2: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayHistoryRow {
    pub time: String,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub battery_a: Option<f64>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DaySwitchRow {
    pub time: String,
    pub mode: String,
    pub reason: String,
}

/// Everything is pre-computed here so the model reasons instead of doing
/// arithmetic.
#[derive(Debug, Clone, Serialize)]
pub struct DayInput {
    pub now: String,
    pub sunset: String,
    pub hours_of_sun_left: f64,
    pub current_mode: String,
    pub soc: f64,
    pub rated_capacity_ah: f64,
    pub battery_v: Option<f64>,
    pub pv_w: Option<f64>,
    pub load_w: Option<f64>,
    pub battery_a: Option<f64>,
    pub smart_load_on: Option<bool>,
    pub sky: String,
    pub dim_minutes: i64,
    pub typical_load_now_w: Option<f64>,
    pub load_is_unusual: bool,
    pub last_90_min: Vec<DayHistoryRow>,
    pub forecast_available: bool,
    pub forecast: Vec<HourOutlookRow>,
    pub recent_days: Vec<RecentDayRow>,
    pub projected_soc_at_sunset: Option<f64>,
    pub projected_full_at: Option<String>,
    pub usual_night_start: String,
    pub tonight_needs_soc: Option<f64>,
    pub projected_sunrise_soc: Option<f64>,
    pub floor_soc: f64,
    pub today_switches: Vec<DaySwitchRow>,
    pub switches_today: u32,
    pub minutes_since_last_switch: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DayDecision {
    pub mode: String,
    pub recheck_minutes: f64,
    pub confidence: f64,
    pub reason: String,
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvDayRow {
    pub date: String,
    pub relative: String,
    pub radiation_kwh_m2: Option<f64>,
    pub sunshine_h: Option<f64>,
    pub cloud_pct: Option<f64>,
    pub rain_prob_pct: Option<f64>,
    pub expected_pv_kwh: Option<f64>,
    pub cooking_day: bool,
    /// What that day's sun leaves for the scooter and the geyser after the
    /// house, the battery's refill, the water tank and, on a cooking day,
    /// the hotplate.
    pub expected_spare_kwh: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadsReading {
    pub time: String,
    pub load_w: Option<f64>,
    pub pv_w: Option<f64>,
    pub soc: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadsHour {
    pub time: String,
    pub sun_w: f64,
    pub house_w: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadsBelief {
    pub load: String,
    pub watts: f64,
    pub since: String,
}

/// The facts behind a solar-loads decision, worked out so the model reasons
/// instead of doing arithmetic. Times are 24-hour "HH:MM" today.
#[derive(Debug, Clone, Serialize)]
pub struct LoadsAiInput {
    pub now: String,
    pub time: String,
    pub sunset: String,
    pub soc: f64,
    pub battery_kwh: f64,
    /// What the battery can lend in the day before it reaches the floor.
    pub battery_lend_kwh: f64,
    pub battery_w: Option<f64>,
    pub day_floor_soc: f64,
    pub full_by_sun_alone: Option<String>,
    pub sunset_soc_sun_alone: f64,
    pub load_w: Option<f64>,
    pub pv_w: Option<f64>,
    pub pv_held_back: bool,
    /// The last half hour's panels as a share of the forecast; the hours
    /// below already carry it.
    pub sky_vs_forecast_pct: Option<f64>,
    pub house_usual_w: f64,
    pub recent: Vec<LoadsReading>,
    pub believed: Vec<LoadsBelief>,
    pub hours: Vec<LoadsHour>,
    pub water: String,
    pub geyser: String,
    pub hotplate: String,
    pub scooter: String,
    pub priority: String,
    pub next_days: Vec<EvDayRow>,
    pub answers: Vec<String>,
    pub previous: Option<String>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoadsRunning {
    pub load: String,
    pub watts: f64,
    #[serde(default)]
    pub since: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoadsRun {
    pub load: String,
    pub start: String,
    pub end: String,
    pub watts: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LoadsNotes {
    #[serde(default)]
    pub water: String,
    #[serde(default)]
    pub geyser: String,
    #[serde(default)]
    pub hotplate: String,
    #[serde(default)]
    pub scooter: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoadsDecision {
    pub running: Vec<LoadsRunning>,
    pub sure: bool,
    #[serde(default)]
    pub question: String,
    pub plan: Vec<LoadsRun>,
    pub scooter_first: bool,
    #[serde(default)]
    pub notes: LoadsNotes,
    pub recheck_minutes: f64,
    pub reason: String,
    #[serde(default)]
    pub model: Option<String>,
}

pub trait AgentRunner: Send + Sync {
    fn run_sun(&self, input: SunInput) -> BoxFuture<'_, Result<SunInfo, AgentError>>;
    fn run_night(&self, input: NightInput) -> BoxFuture<'_, Result<NightPlan, AgentError>>;
    fn run_day(&self, input: DayInput) -> BoxFuture<'_, Result<DayDecision, AgentError>>;
    fn run_loads(&self, input: LoadsAiInput) -> BoxFuture<'_, Result<LoadsDecision, AgentError>>;
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
        Box::pin(async move { run_script("sun.js", &input, &[]).await })
    }

    fn run_night(&self, input: NightInput) -> BoxFuture<'_, Result<NightPlan, AgentError>> {
        Box::pin(async move {
            run_script("night.js", &input, &model_keys()?).await
        })
    }

    fn run_day(&self, input: DayInput) -> BoxFuture<'_, Result<DayDecision, AgentError>> {
        Box::pin(async move { run_script("day.js", &input, &model_keys()?).await })
    }

    fn run_loads(&self, input: LoadsAiInput) -> BoxFuture<'_, Result<LoadsDecision, AgentError>> {
        Box::pin(async move { run_script("loads.js", &input, &model_keys()?).await })
    }
}

/// The API keys the agent script needs, as (environment variable, Settings
/// label, keychain reader). Groq is tried first, Gemini as the backup.
const MODEL_KEYS: [(&str, &str, fn() -> Result<Option<String>, String>); 2] = [
    ("GROQ_API_KEY", "Groq", crate::storage::get_groq_api_key),
    ("GEMINI_API_KEY", "Gemini", crate::storage::get_gemini_api_key),
];

/// Why automation can't run yet, if a required key is missing.
pub fn missing_key_reason() -> Option<String> {
    MODEL_KEYS
        .iter()
        .find(|(_, _, read)| read().ok().flatten().is_none())
        .map(|(_, label, _)| format!("Set a {label} API key in Settings"))
}

fn model_keys() -> Result<Vec<(&'static str, String)>, AgentError> {
    MODEL_KEYS
        .iter()
        .map(|(name, _, read)| {
            let key = read().map_err(AgentError::ScriptFailed)?;
            key.map(|key| (*name, key)).ok_or_else(|| AgentError::ScriptFailed(format!("{name} is not set")))
        })
        .collect()
}

fn agent_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../agent"))
}

async fn run_script<I, O>(script: &str, input: &I, keys: &[(&str, String)]) -> Result<O, AgentError>
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
        match run_once(script, &payload, keys).await {
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

async fn run_once(script: &str, payload: &[u8], keys: &[(&str, String)]) -> Result<Vec<u8>, AgentError> {
    let mut last_error = AgentError::NodeNotFound;
    for node in NODE_CANDIDATES {
        let mut command = Command::new(node);
        command
            .current_dir(agent_dir())
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .envs(keys.iter().map(|(name, value)| (*name, value.as_str())));

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
        for line in stderr.lines().filter(|line| line.contains(" failed after ") || line.starts_with("Got a response from ")) {
            tracing::info!("{script}: {}", line.chars().take(300).collect::<String>());
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
        assert_eq!(classify_failure("ApiError: {\"error\":{\"code\":400,\"message\":\"API key not valid\"}}"), "an AI provider rejected its API key");
        assert_eq!(classify_failure("got status: 429 RESOURCE_EXHAUSTED"), "the AI quota or rate limit was reached");
        assert_eq!(classify_failure("got status: 503 UNAVAILABLE. The model is overloaded"), "the AI models are busy or down");
        assert_eq!(classify_failure("SyntaxError: Unexpected token"), "the AI script failed — see the log");
    }

    #[test]
    fn mixed_model_failures_report_the_overload() {
        let mixed = "gemini-3.5-flash-lite failed after 624ms: 503 {\"error\":{\"code\":503,\"status\":\"UNAVAILABLE\"}}\n\
                     gemini-3.1-flash-lite failed after 900ms: TypeError fetch failed ECONNRESET at node:internal/process/task_queues";
        assert_eq!(classify_failure(mixed), "the AI models are busy or down");
        let timed_out = "gemma-4-26b-a4b-it failed after 15001ms: TimeoutError The operation was aborted due to timeout";
        assert_eq!(classify_failure(timed_out), "the AI models are busy or down");
        let offline = "TypeError: fetch failed at node:internal/deps/undici ENOTFOUND generativelanguage.googleapis.com";
        assert_eq!(classify_failure(offline), "no internet connection", "a node stack trace isn't a server error");
        assert_eq!(classify_failure("ApiError: {\"error\":{\"code\":500,\"status\":\"INTERNAL\"}}"), "the AI models are busy or down");
    }
}
