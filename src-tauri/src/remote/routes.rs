use std::convert::Infallible;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;

use super::{PairError, RemoteState};
use crate::{automation, battery, energy, inverter};

const DESKTOP_ONLY: &[&str] = &[
    "scan_bms_devices",
    "connect_bms_device",
    "disconnect_bms_device",
    "reconnect_saved_bms",
    "set_saved_bms_device",
    "update_solar_settings",
    "open_records_folder",
    "send_test_notification",
    "get_remote_status",
    "set_remote_enabled",
    "regenerate_remote_pin",
];

#[derive(Clone)]
pub(super) struct Ctx {
    pub app: AppHandle,
    pub remote: RemoteState,
    pub shutdown: watch::Receiver<bool>,
}

pub(super) fn router(ctx: Ctx) -> Router {
    Router::new()
        .route("/api/pair", post(pair))
        .route("/api/session", get(session))
        .route("/api/invoke/{command}", post(invoke))
        .route("/api/events", get(events))
        .fallback(asset)
        .with_state(ctx)
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "This phone is not paired. Enter the PIN from the desktop app.").into_response()
}

#[derive(Deserialize)]
struct PairBody {
    pin: String,
}

async fn pair(State(ctx): State<Ctx>, Json(body): Json<PairBody>) -> Response {
    match ctx.remote.pair(&ctx.app, body.pin.trim()).await {
        Ok(token) => Json(json!({ "token": token })).into_response(),
        Err(PairError::WrongPin) => (StatusCode::UNAUTHORIZED, "That PIN is not right.").into_response(),
        Err(PairError::Locked(seconds)) => (
            StatusCode::TOO_MANY_REQUESTS,
            format!("Too many wrong tries. Wait {seconds}s and try again."),
        )
            .into_response(),
        Err(PairError::Save(error)) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

async fn session(State(ctx): State<Ctx>, headers: HeaderMap) -> Response {
    if ctx.remote.is_authorized(bearer(&headers)).await {
        Json(json!({ "ok": true })).into_response()
    } else {
        unauthorized()
    }
}

async fn invoke(State(ctx): State<Ctx>, Path(command): Path<String>, headers: HeaderMap, body: Bytes) -> Response {
    if !ctx.remote.is_authorized(bearer(&headers)).await {
        return unauthorized();
    }
    if DESKTOP_ONLY.contains(&command.as_str()) {
        return (StatusCode::FORBIDDEN, "This can only be done on the desktop app.").into_response();
    }
    let args: Value = if body.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
        }
    };
    match dispatch(&ctx.app, &command, &args).await {
        Ok(Some(value)) => Json(value).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, format!("Unknown command {command}.")).into_response(),
        Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error).into_response(),
    }
}

fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null)).map_err(|error| format!("Bad `{key}`: {error}"))
}

fn to_json<T: Serialize>(result: Result<T, String>) -> Result<Option<Value>, String> {
    let value = result?;
    serde_json::to_value(value).map(Some).map_err(|error| error.to_string())
}

async fn dispatch(app: &AppHandle, command: &str, args: &Value) -> Result<Option<Value>, String> {
    use automation::commands as auto;
    use battery::commands as bat;
    use energy::commands as en;
    use inverter::commands as inv;

    match command {
        "get_battery_state" => to_json(bat::get_battery_state(app.state()).await),
        "get_battery_connection" => to_json(bat::get_battery_connection(app.state()).await),
        "get_saved_bms_device" => to_json(bat::get_saved_bms_device(app.clone()).await),

        "get_inverter_snapshot" => to_json(inv::get_inverter_snapshot(app.state(), app.state(), app.clone()).await),
        "get_inverter_settings" => to_json(inv::get_inverter_settings(app.state(), arg(args, "deviceId")?).await),
        "set_output_priority" => {
            to_json(inv::set_output_priority(app.state(), arg(args, "mode")?, arg(args, "deviceId")?).await)
        }
        "set_charger_priority" => {
            to_json(inv::set_charger_priority(app.state(), arg(args, "mode")?, arg(args, "deviceId")?).await)
        }
        "set_ac_input_range" => {
            to_json(inv::set_ac_input_range(app.state(), arg(args, "enabled")?, arg(args, "deviceId")?).await)
        }
        "set_grid_feed_in" => to_json(inv::set_grid_feed_in(app.state(), arg(args, "enabled")?, arg(args, "deviceId")?).await),
        "set_smart_load" => to_json(inv::set_smart_load(app.state(), arg(args, "enabled")?, arg(args, "deviceId")?).await),
        "set_low_battery_cutoff_voltage" => to_json(
            inv::set_low_battery_cutoff_voltage(app.state(), arg(args, "value")?, arg(args, "deviceId")?).await,
        ),
        "set_high_cutoff_voltage" => {
            to_json(inv::set_high_cutoff_voltage(app.state(), arg(args, "value")?, arg(args, "deviceId")?).await)
        }
        "set_low_dc_cutoff_soc" => {
            to_json(inv::set_low_dc_cutoff_soc(app.state(), arg(args, "value")?, arg(args, "deviceId")?).await)
        }
        "set_max_total_charge_current" => to_json(
            inv::set_max_total_charge_current(app.state(), arg(args, "value")?, arg(args, "deviceId")?).await,
        ),
        "set_max_utility_charge_current" => to_json(
            inv::set_max_utility_charge_current(app.state(), arg(args, "value")?, arg(args, "deviceId")?).await,
        ),
        "get_device_details" => to_json(inv::get_device_details(app.state(), arg(args, "deviceId")?).await),
        "get_solar_settings" => to_json(inv::get_solar_settings(app.clone()).await),

        "get_automation_status" => to_json(auto::get_automation_status(app.state()).await),
        "get_automation_config" => to_json(auto::get_automation_config(app.state()).await),
        "update_automation_config" => {
            to_json(auto::update_automation_config(app.clone(), app.state(), arg(args, "config")?).await)
        }
        "force_automation_check" => to_json(auto::force_automation_check(app.state()).await),
        "get_automation_insights" => to_json(auto::get_automation_insights(app.state()).await),
        "get_automation_decisions" => to_json(auto::get_automation_decisions(app.state(), arg(args, "limit")?).await),

        "get_energy_summary" => to_json(en::get_energy_summary(app.state()).await),
        "set_bill_reading" => to_json(en::set_bill_reading(app.clone(), app.state(), arg(args, "day")?, arg(args, "time")?).await),
        "set_standby_watts" => to_json(en::set_standby_watts(app.clone(), app.state(), arg(args, "watts")?).await),
        "assign_meter_range" => to_json(
            en::assign_meter_range(app.clone(), app.state(), arg(args, "from")?, arg(args, "to")?, arg(args, "meter")?).await,
        ),
        "remove_meter_assignment" => to_json(en::remove_meter_assignment(app.clone(), app.state(), arg(args, "from")?).await),
        "set_active_meter" => {
            to_json(en::set_active_meter(app.clone(), app.state(), arg(args, "meter")?, arg(args, "at")?).await)
        }

        _ => Ok(None),
    }
}

#[derive(Deserialize)]
struct EventsQuery {
    token: Option<String>,
}

async fn events(State(ctx): State<Ctx>, Query(query): Query<EventsQuery>) -> Response {
    if !ctx.remote.is_authorized(query.token.as_deref()).await {
        return unauthorized();
    }
    let receiver = crate::events::subscribe_remote();
    let stream = futures_util::stream::unfold((receiver, ctx.shutdown.clone()), |(mut receiver, mut shutdown)| async move {
        loop {
            let message = tokio::select! {
                _ = shutdown.wait_for(|stop| *stop) => return None,
                message = receiver.recv() => message,
            };
            match message {
                Ok((name, data)) => {
                    let event = Event::default().event(name).data(data);
                    return Some((Ok::<_, Infallible>(event), (receiver, shutdown)));
                }
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

async fn asset(State(ctx): State<Ctx>, uri: Uri) -> Response {
    let path = uri.path();
    if path.starts_with("/api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    match ctx.app.asset_resolver().get(path.to_string()) {
        Some(asset) => {
            let cache = if path.starts_with("/assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            (
                [(header::CONTENT_TYPE, asset.mime_type().to_string()), (header::CACHE_CONTROL, cache.to_string())],
                asset.bytes().to_vec(),
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            "The phone view is served from the built app. During development open the Vite dev server with --host instead.",
        )
            .into_response(),
    }
}
