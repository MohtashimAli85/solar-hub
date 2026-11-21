use std::time::Duration;

use serde::{Deserialize, Serialize};

const TIMEOUT: Duration = Duration::from_secs(5);

/// The geyser panel's buttons as the ESP32 firmware names them. 800 and 1200
/// each toggle one heating element; 2000 W is both elements on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Button {
    #[serde(rename = "power")]
    Power,
    #[serde(rename = "800")]
    W800,
    #[serde(rename = "1200")]
    W1200,
    #[serde(rename = "up")]
    TempUp,
    #[serde(rename = "down")]
    TempDown,
}

impl Button {
    pub const ALL: [Button; 5] = [Button::Power, Button::W800, Button::W1200, Button::TempUp, Button::TempDown];

    pub fn as_str(self) -> &'static str {
        match self {
            Button::Power => "power",
            Button::W800 => "800",
            Button::W1200 => "1200",
            Button::TempUp => "up",
            Button::TempDown => "down",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Button::Power => "On/Off",
            Button::W800 => "800 W",
            Button::W1200 => "1200 W",
            Button::TempUp => "Temperature up",
            Button::TempDown => "Temperature down",
        }
    }

    pub fn parse(name: &str) -> Option<Button> {
        let name = name.trim();
        Self::ALL.into_iter().find(|button| button.as_str().eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteStatus {
    pub ip: Option<String>,
    pub rssi: Option<i32>,
    pub uptime_s: Option<u64>,
    #[serde(default)]
    pub last_button: Option<String>,
    pub last_pressed_s_ago: Option<u64>,
    #[serde(default)]
    pub last_heard: Option<String>,
    pub last_heard_s_ago: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PressOutcome {
    Pressed,
    Unsure(String),
    Failed(String),
}

#[derive(Deserialize)]
struct Reply {
    error: Option<String>,
}

#[derive(Clone)]
pub struct GeyserRemote {
    base: String,
    http: reqwest::Client,
}

impl GeyserRemote {
    pub fn new(host: &str) -> Self {
        let http = reqwest::Client::builder().timeout(TIMEOUT).build().unwrap_or_default();
        Self { base: base_url(host), http }
    }

    pub async fn status(&self) -> Result<RemoteStatus, String> {
        let response = self
            .http
            .get(format!("{}/status", self.base))
            .send()
            .await
            .map_err(|error| self.unreachable(&error))?;
        let code = response.status().as_u16();
        let body = response.text().await.map_err(|error| self.unreachable(&error))?;
        parse_status(code, &body)
    }

    pub async fn press(&self, button: Button) -> Result<(), String> {
        match self.try_press(button).await {
            PressOutcome::Pressed => Ok(()),
            PressOutcome::Unsure(error) | PressOutcome::Failed(error) => Err(error),
        }
    }

    /// A press whose request reached the ESP32 but timed out waiting for the
    /// reply may still have gone out: the firmware answers after sending the IR.
    pub async fn try_press(&self, button: Button) -> PressOutcome {
        let response = match self.http.post(press_url(&self.base, button)).send().await {
            Ok(response) => response,
            Err(error) if error.is_timeout() && !error.is_connect() => return PressOutcome::Unsure(self.unreachable(&error)),
            Err(error) => return PressOutcome::Failed(self.unreachable(&error)),
        };
        let code = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        match check_reply(code, &body) {
            Ok(()) => PressOutcome::Pressed,
            Err(error) => PressOutcome::Failed(error),
        }
    }

    fn unreachable(&self, error: &reqwest::Error) -> String {
        let mut cause = error.to_string();
        let mut source = std::error::Error::source(error);
        while let Some(inner) = source {
            cause.push_str(&format!(": {inner}"));
            source = inner.source();
        }
        tracing::warn!("geyser remote request to {} failed: {cause}", self.base);
        if error.is_timeout() {
            format!("geyser remote at {} didn't answer within {} s", self.base, TIMEOUT.as_secs())
        } else {
            format!("geyser remote not reachable at {}", self.base)
        }
    }
}

fn base_url(host: &str) -> String {
    let host = host.trim();
    let lower = host.to_ascii_lowercase();
    let host = ["http://", "https://"]
        .iter()
        .find(|scheme| lower.starts_with(*scheme))
        .map_or(host, |scheme| &host[scheme.len()..]);
    format!("http://{}", host.trim_end_matches('/'))
}

fn press_url(base: &str, button: Button) -> String {
    format!("{base}/press?button={}", button.as_str())
}

fn parse_status(code: u16, body: &str) -> Result<RemoteStatus, String> {
    if !(200..300).contains(&code) {
        return Err(format!("geyser remote answered {code}"));
    }
    serde_json::from_str(body).map_err(|_| "geyser remote sent a reply the app doesn't understand".to_string())
}

fn check_reply(code: u16, body: &str) -> Result<(), String> {
    if (200..300).contains(&code) {
        return Ok(());
    }
    match serde_json::from_str::<Reply>(body).ok().and_then(|reply| reply.error) {
        Some(error) => Err(format!("geyser remote answered {code}: {error}")),
        None => Err(format!("geyser remote answered {code}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Query;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    const FIRMWARE_STATUS: &str = r#"{"ok":true,"ip":"192.168.1.100","rssi":-68,"uptime_s":3600,"last_button":"800","last_pressed_s_ago":12,"last_heard":"800","last_heard_s_ago":12,"learning":false,"learn_result":"","learned":0}"#;

    #[test]
    fn buttons_round_trip_through_their_names() {
        for button in Button::ALL {
            assert_eq!(Button::parse(button.as_str()), Some(button));
            assert_eq!(serde_json::to_value(button).unwrap(), json!(button.as_str()));
        }
        assert_eq!(Button::parse(" UP "), Some(Button::TempUp));
        assert_eq!(Button::parse("2000"), None);
    }

    #[test]
    fn hosts_become_plain_http_base_urls() {
        assert_eq!(base_url("geyser.local"), "http://geyser.local");
        assert_eq!(base_url(" http://192.168.1.100/ "), "http://192.168.1.100");
        assert_eq!(base_url("HTTPS://geyser.local:8080"), "http://geyser.local:8080");
        assert_eq!(press_url("http://192.168.1.100", Button::W1200), "http://192.168.1.100/press?button=1200");
    }

    #[test]
    fn status_parses_the_firmware_reply() {
        let status = parse_status(200, FIRMWARE_STATUS).unwrap();
        assert_eq!(status.ip.as_deref(), Some("192.168.1.100"));
        assert_eq!(status.rssi, Some(-68));
        assert_eq!(status.uptime_s, Some(3600));
        assert_eq!(status.last_button.as_deref(), Some("800"));
        assert_eq!(status.last_heard_s_ago, Some(12));

        let fresh = parse_status(200, r#"{"ok":true,"ip":"192.168.1.100","rssi":-70,"uptime_s":5,"last_button":"","last_pressed_s_ago":null,"last_heard":"","last_heard_s_ago":null}"#).unwrap();
        assert_eq!(fresh.last_pressed_s_ago, None);
        assert!(parse_status(500, FIRMWARE_STATUS).is_err());
        assert!(parse_status(200, "<html>").is_err());
    }

    #[test]
    fn errors_carry_the_firmware_message() {
        assert_eq!(
            check_reply(400, r#"{"ok":false,"error":"unknown button"}"#),
            Err("geyser remote answered 400: unknown button".into())
        );
        assert_eq!(check_reply(502, "oops"), Err("geyser remote answered 502".into()));
        assert_eq!(check_reply(200, r#"{"ok":true,"button":"power"}"#), Ok(()));
    }

    #[tokio::test]
    async fn talks_to_a_device_over_http() {
        async fn press(Query(params): Query<BTreeMap<String, String>>) -> (StatusCode, Json<Value>) {
            match params.get("button").map(String::as_str) {
                Some("power" | "800" | "1200" | "up" | "down") => (StatusCode::OK, Json(json!({"ok": true}))),
                _ => (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "error": "unknown button"}))),
            }
        }
        let router = Router::new()
            .route("/status", get(|| async { FIRMWARE_STATUS }))
            .route("/press", post(press));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let remote = GeyserRemote::new(&address.to_string());
        assert_eq!(remote.status().await.unwrap().rssi, Some(-68));
        for button in Button::ALL {
            assert_eq!(remote.press(button).await, Ok(()));
        }
    }

    #[tokio::test]
    async fn an_unreachable_device_says_so() {
        let error = GeyserRemote::new("127.0.0.1:1").status().await.unwrap_err();
        assert_eq!(error, "geyser remote not reachable at http://127.0.0.1:1");
    }
}
