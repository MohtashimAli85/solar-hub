use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Button {
    Power,
    W800,
    W1200,
    W2000,
    TempUp,
    TempDown,
}

impl Button {
    pub const ALL: [Button; 6] = [
        Button::Power,
        Button::W800,
        Button::W1200,
        Button::W2000,
        Button::TempUp,
        Button::TempDown,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Button::Power => "power",
            Button::W800 => "w800",
            Button::W1200 => "w1200",
            Button::W2000 => "w2000",
            Button::TempUp => "temp_up",
            Button::TempDown => "temp_down",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Button::Power => "On/Off",
            Button::W800 => "800 W",
            Button::W1200 => "1200 W",
            Button::W2000 => "2000 W",
            Button::TempUp => "Temperature up",
            Button::TempDown => "Temperature down",
        }
    }

    pub fn parse(name: &str) -> Option<Button> {
        let name = name.trim();
        Self::ALL.into_iter().find(|button| button.as_str().eq_ignore_ascii_case(name))
    }

    pub fn for_watts(watts: u32) -> Option<Button> {
        match watts {
            800 => Some(Button::W800),
            1200 => Some(Button::W1200),
            2000 => Some(Button::W2000),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LearnResult {
    pub button: String,
    pub ok: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub length: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemoteStatus {
    #[serde(default)]
    pub learned: BTreeMap<String, bool>,
    pub learning: Option<String>,
    #[serde(default)]
    pub last_learn: Option<LearnResult>,
    pub last_sent: Option<String>,
    pub uptime_s: Option<u64>,
    pub rssi: Option<i32>,
}

impl RemoteStatus {
    pub fn is_learned(&self, button: Button) -> bool {
        self.learned.get(button.as_str()).copied().unwrap_or(false)
    }
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

    pub async fn learn(&self, button: Button) -> Result<(), String> {
        self.post("learn", button).await
    }

    pub async fn send(&self, button: Button) -> Result<(), String> {
        self.post("send", button).await
    }

    pub async fn forget(&self, button: Button) -> Result<(), String> {
        self.post("forget", button).await
    }

    async fn post(&self, action: &str, button: Button) -> Result<(), String> {
        let response = self
            .http
            .post(button_url(&self.base, action, button))
            .send()
            .await
            .map_err(|error| self.unreachable(&error))?;
        let code = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        check_reply(code, &body, button)
    }

    fn unreachable(&self, error: &reqwest::Error) -> String {
        tracing::warn!("geyser remote request to {} failed: {error}", self.base);
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

fn button_url(base: &str, action: &str, button: Button) -> String {
    format!("{base}/{action}?button={}", button.as_str())
}

fn parse_status(code: u16, body: &str) -> Result<RemoteStatus, String> {
    if !(200..300).contains(&code) {
        return Err(format!("geyser remote answered {code}"));
    }
    serde_json::from_str(body).map_err(|_| "geyser remote sent a reply the app doesn't understand".to_string())
}

fn check_reply(code: u16, body: &str, button: Button) -> Result<(), String> {
    if (200..300).contains(&code) {
        return Ok(());
    }
    let error = serde_json::from_str::<Reply>(body).ok().and_then(|reply| reply.error);
    match (code, error.as_deref()) {
        (404, Some("not learned")) => Err(format!("button not learned yet: {}", button.label())),
        (409, _) => Err("the geyser remote is still learning a button — try again in 20 seconds".into()),
        (_, Some(error)) => Err(format!("geyser remote answered {code}: {error}")),
        _ => Err(format!("geyser remote answered {code}")),
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

    const FIRMWARE_STATUS: &str = r#"{"ok":true,"learned":{"power":true,"w800":true,"w1200":false,"w2000":false,"temp_up":false,"temp_down":false},"learning":null,"last_learn":{"button":"w800","ok":true,"error":null,"length":243},"last_sent":"w800","last_sent_ms_ago":1234,"uptime_s":3600,"rssi":-61,"version":"1"}"#;

    #[test]
    fn buttons_round_trip_through_their_names() {
        for button in Button::ALL {
            assert_eq!(Button::parse(button.as_str()), Some(button));
            assert_eq!(serde_json::to_value(button).unwrap(), json!(button.as_str()));
        }
        assert_eq!(Button::parse(" TEMP_UP "), Some(Button::TempUp));
        assert_eq!(Button::parse("w1000"), None);
    }

    #[test]
    fn watts_pick_the_power_buttons() {
        assert_eq!(Button::for_watts(800), Some(Button::W800));
        assert_eq!(Button::for_watts(1200), Some(Button::W1200));
        assert_eq!(Button::for_watts(2000), Some(Button::W2000));
        assert_eq!(Button::for_watts(1000), None);
    }

    #[test]
    fn hosts_become_plain_http_base_urls() {
        assert_eq!(base_url("solarhub-geyser.local"), "http://solarhub-geyser.local");
        assert_eq!(base_url(" http://192.168.1.50/ "), "http://192.168.1.50");
        assert_eq!(base_url("HTTPS://geyser.local:8080"), "http://geyser.local:8080");
        assert_eq!(
            button_url("http://solarhub-geyser.local", "send", Button::W1200),
            "http://solarhub-geyser.local/send?button=w1200"
        );
    }

    #[test]
    fn status_parses_the_firmware_reply() {
        let status = parse_status(200, FIRMWARE_STATUS).unwrap();
        assert!(status.is_learned(Button::Power));
        assert!(!status.is_learned(Button::W2000));
        assert_eq!(status.learning, None);
        assert_eq!(status.last_sent.as_deref(), Some("w800"));
        assert_eq!(status.uptime_s, Some(3600));
        assert_eq!(status.rssi, Some(-61));
        assert_eq!(status.last_learn.unwrap().length, Some(243));

        let learning = parse_status(200, r#"{"ok":true,"learned":{},"learning":"power","last_sent":null,"uptime_s":5,"rssi":-70}"#).unwrap();
        assert_eq!(learning.learning.as_deref(), Some("power"));
        assert_eq!(learning.last_learn, None);
        assert!(parse_status(500, FIRMWARE_STATUS).is_err());
        assert!(parse_status(200, "<html>").is_err());
    }

    #[test]
    fn errors_name_the_button() {
        assert_eq!(
            check_reply(404, r#"{"ok":false,"error":"not learned"}"#, Button::W800),
            Err("button not learned yet: 800 W".into())
        );
        assert_eq!(
            check_reply(400, r#"{"ok":false,"error":"unknown button"}"#, Button::Power),
            Err("geyser remote answered 400: unknown button".into())
        );
        assert!(check_reply(409, r#"{"ok":false,"error":"learning"}"#, Button::Power).unwrap_err().contains("still learning"));
        assert_eq!(check_reply(200, r#"{"ok":true}"#, Button::Power), Ok(()));
    }

    #[tokio::test]
    async fn talks_to_a_device_over_http() {
        async fn send(Query(params): Query<BTreeMap<String, String>>) -> (StatusCode, Json<Value>) {
            match params.get("button").map(String::as_str) {
                Some("w800") => (StatusCode::OK, Json(json!({"ok": true}))),
                _ => (StatusCode::NOT_FOUND, Json(json!({"ok": false, "error": "not learned"}))),
            }
        }
        let router = Router::new()
            .route("/status", get(|| async { FIRMWARE_STATUS }))
            .route("/send", post(send));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let remote = GeyserRemote::new(&address.to_string());
        assert!(remote.status().await.unwrap().is_learned(Button::W800));
        assert_eq!(remote.send(Button::W800).await, Ok(()));
        assert_eq!(remote.send(Button::W2000).await, Err("button not learned yet: 2000 W".into()));
    }

    #[tokio::test]
    async fn an_unreachable_device_says_so() {
        let error = GeyserRemote::new("127.0.0.1:1").status().await.unwrap_err();
        assert_eq!(error, "geyser remote not reachable at http://127.0.0.1:1");
    }
}
