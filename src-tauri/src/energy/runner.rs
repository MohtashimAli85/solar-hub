use std::collections::HashSet;
use std::time::Duration;

use chrono::{NaiveDate, NaiveTime};
use chrono_tz::Tz;
use tauri::AppHandle;

use super::units::{compute_day, day_window, unit_points, UnitPoint, HISTORY_KEYS};
use super::{local_now, Backfill, EnergyState};
use crate::events;
use crate::inverter::client::SolarClient;
use crate::inverter::InverterError;

const REFRESH_EVERY: Duration = Duration::from_secs(10 * 60);
const BACKFILL_PAUSE: Duration = Duration::from_secs(1);
const BACKFILL_EMIT_EVERY: usize = 5;
/// Give the logger a few minutes past midnight before a day counts as finished.
const DAY_SETTLE_MINUTES: u32 = 15;
/// An empty recent day may just be the cloud catching up, so it isn't saved
/// until it is this old.
const EMPTY_DAY_SAVE_AFTER_DAYS: i64 = 2;

pub fn spawn(state: EnergyState, client: SolarClient, app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            refresh(&state, &client, &app).await;
            tokio::select! {
                _ = tokio::time::sleep(REFRESH_EVERY) => {}
                _ = state.wake.notified() => {}
            }
        }
    });
}

async fn publish(state: &EnergyState, app: &AppHandle) {
    let summary = state.rebuild().await;
    events::emit(app, events::ENERGY_UPDATED, summary);
}

fn friendly(error: &InverterError) -> String {
    match error {
        InverterError::Config(message) => message.clone(),
        InverterError::Auth(_) => "The inverter cloud rejected the login; check Settings.".into(),
        _ => "Couldn't reach the inverter cloud; showing the last numbers.".into(),
    }
}

async fn fetch_day(client: &SolarClient, tz: Tz, day: NaiveDate) -> Result<Vec<UnitPoint>, InverterError> {
    let (from, to) = day_window(day, tz).ok_or_else(|| InverterError::TimeZone("no such local day".into()))?;
    let points = client.read_attribute_history(from, to, &HISTORY_KEYS).await?;
    Ok(unit_points(&points, tz))
}

async fn refresh(state: &EnergyState, client: &SolarClient, app: &AppHandle) {
    let tz = client.time_zone().await;
    if !client.is_configured() {
        state.set_tz(tz, false).await;
        publish(state, app).await;
        return;
    }
    state.set_tz(tz, true).await;
    let now = local_now(tz);
    let today = now.date();

    match fetch_day(client, tz, today).await {
        Ok(points) => {
            state.set_today(today, points, local_now(tz)).await;
            state.set_error(None).await;
        }
        Err(error) => {
            tracing::warn!("energy: today's history failed: {error}");
            state.set_error(Some(friendly(&error))).await;
        }
    }
    publish(state, app).await;

    let reading = state.config().await.reading();
    let previous_start = reading.previous_period_start(now);
    let from = previous_start.date();
    for start in [previous_start, reading.period_start(now)] {
        let day = start.date();
        if day == today || state.has_reading_day(day).await {
            continue;
        }
        match fetch_day(client, tz, day).await {
            Ok(points) => state.set_reading_day_points(day, points).await,
            Err(error) => tracing::warn!("energy: reading day {day} failed: {error}"),
        }
    }
    let settle = NaiveTime::from_hms_opt(0, DAY_SETTLE_MINUTES, 0).unwrap_or(NaiveTime::MIN);
    let finished_back = if now.time() >= settle { 1 } else { 2 };
    let last_finished = today - chrono::Duration::days(finished_back);
    let saved: HashSet<NaiveDate> = state.records.days_between(from, last_finished).iter().map(|day| day.date).collect();
    let missing: Vec<NaiveDate> = from
        .iter_days()
        .take_while(|day| *day <= last_finished)
        .filter(|day| !saved.contains(day))
        .collect();
    if missing.is_empty() {
        return;
    }

    let switches = state.records.meter_switches();
    let total = missing.len() as u32;
    for (index, day) in missing.iter().rev().enumerate() {
        state.set_backfill(Some(Backfill { done: index as u32, total })).await;
        if index % BACKFILL_EMIT_EVERY == 0 {
            publish(state, app).await;
        }
        match fetch_day(client, tz, *day).await {
            Ok(points) => {
                let day_end = day.succ_opt().unwrap_or(*day).and_time(NaiveTime::MIN);
                let (units, _) = compute_day(&points, &switches, *day, day_end);
                let old_enough = (today - *day).num_days() > EMPTY_DAY_SAVE_AFTER_DAYS;
                if units.has_data() || old_enough {
                    if let Err(error) = state.records.append_day(&units) {
                        tracing::warn!("energy: couldn't save {day}: {error}");
                    }
                }
            }
            Err(error) => {
                tracing::warn!("energy: history for {day} failed: {error}");
                state.set_error(Some(friendly(&error))).await;
                break;
            }
        }
        tokio::time::sleep(BACKFILL_PAUSE).await;
    }
    state.set_backfill(None).await;
    publish(state, app).await;
}
