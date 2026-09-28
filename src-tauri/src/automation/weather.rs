use chrono::{Duration, NaiveDate, NaiveDateTime, Timelike};
use serde::Serialize;
use serde_json::Value;

use crate::automation::history::DaySolar;

const FORECAST_URL: &str = "https://api.open-meteo.com/v1/forecast";
const MJ_PER_KWH: f64 = 3.6;
const PERFORMANCE_RATIO: f64 = 0.75;

#[derive(Debug, Clone, PartialEq)]
pub struct HourWeather {
    /// Open-Meteo reports radiation as the mean over the hour *ending* here.
    pub at: NaiveDateTime,
    pub temp_c: Option<f64>,
    pub cloud_pct: Option<f64>,
    pub radiation_w_m2: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DayWeather {
    pub date: NaiveDate,
    pub radiation_kwh_m2: Option<f64>,
    pub sunshine_h: Option<f64>,
    pub rain_prob_pct: Option<f64>,
    pub temp_min_c: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub hourly: Vec<HourWeather>,
    pub daily: Vec<DayWeather>,
}

pub async fn fetch(http: &reqwest::Client, latitude: f64, longitude: f64) -> Result<Forecast, String> {
    let round = |value: f64| (value * 100.0).round() / 100.0;
    let url = format!(
        "{FORECAST_URL}?latitude={}&longitude={}&hourly=temperature_2m,cloud_cover,shortwave_radiation\
         &daily=shortwave_radiation_sum,sunshine_duration,precipitation_probability_max,temperature_2m_min\
         &timezone=auto&past_days=7&forecast_days=2",
        round(latitude),
        round(longitude)
    );
    let response = http.get(url).send().await.map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("open-meteo returned {}", response.status()));
    }
    let json: Value = response.json().await.map_err(|error| error.to_string())?;
    parse(&json)
}

fn series(block: &Value, key: &str) -> Vec<Option<f64>> {
    block
        .get(key)
        .and_then(Value::as_array)
        .map(|values| values.iter().map(Value::as_f64).collect())
        .unwrap_or_default()
}

fn at_index(values: &[Option<f64>], index: usize) -> Option<f64> {
    values.get(index).copied().flatten()
}

pub fn parse(json: &Value) -> Result<Forecast, String> {
    let hourly_block = json.get("hourly").ok_or("forecast has no hourly block")?;
    let daily_block = json.get("daily").ok_or("forecast has no daily block")?;

    let temps = series(hourly_block, "temperature_2m");
    let clouds = series(hourly_block, "cloud_cover");
    let radiation = series(hourly_block, "shortwave_radiation");
    let hourly = hourly_block
        .get("time")
        .and_then(Value::as_array)
        .ok_or("forecast has no hourly times")?
        .iter()
        .enumerate()
        .filter_map(|(index, time)| {
            let at = NaiveDateTime::parse_from_str(time.as_str()?, "%Y-%m-%dT%H:%M").ok()?;
            Some(HourWeather {
                at,
                temp_c: at_index(&temps, index),
                cloud_pct: at_index(&clouds, index),
                radiation_w_m2: at_index(&radiation, index),
            })
        })
        .collect();

    let radiation_sum = series(daily_block, "shortwave_radiation_sum");
    let sunshine = series(daily_block, "sunshine_duration");
    let rain = series(daily_block, "precipitation_probability_max");
    let temp_min = series(daily_block, "temperature_2m_min");
    let daily = daily_block
        .get("time")
        .and_then(Value::as_array)
        .ok_or("forecast has no daily times")?
        .iter()
        .enumerate()
        .filter_map(|(index, time)| {
            Some(DayWeather {
                date: NaiveDate::parse_from_str(time.as_str()?, "%Y-%m-%d").ok()?,
                radiation_kwh_m2: at_index(&radiation_sum, index).map(|mj| mj / MJ_PER_KWH),
                sunshine_h: at_index(&sunshine, index).map(|seconds| seconds / 3600.0),
                rain_prob_pct: at_index(&rain, index),
                temp_min_c: at_index(&temp_min, index),
            })
        })
        .collect();

    Ok(Forecast { hourly, daily })
}

impl Forecast {
    pub fn day(&self, date: NaiveDate) -> Option<&DayWeather> {
        self.daily.iter().find(|day| day.date == date)
    }

    /// Mean radiation (W/m²) during the hour that contains `at`.
    pub fn radiation_during(&self, at: NaiveDateTime) -> Option<f64> {
        let hour_end = at.date().and_hms_opt(at.hour(), 0, 0)? + Duration::hours(1);
        self.hourly.iter().find(|h| h.at == hour_end).and_then(|h| h.radiation_w_m2)
    }

    pub fn hours_between(&self, from: NaiveDateTime, to: NaiveDateTime) -> Vec<&HourWeather> {
        self.hourly.iter().filter(|h| h.at > from && h.at <= to).collect()
    }

    fn daytime_cloud_avg(&self, date: NaiveDate) -> Option<f64> {
        let clouds: Vec<f64> = self
            .hourly
            .iter()
            .filter(|h| h.at.date() == date && (9..=17).contains(&h.at.hour()))
            .filter_map(|h| h.cloud_pct)
            .collect();
        (!clouds.is_empty()).then(|| clouds.iter().sum::<f64>() / clouds.len() as f64)
    }

    /// Lowest temperature from 21:00 on `night` to 06:00 the next morning.
    pub fn night_min_temp(&self, night: NaiveDate) -> Option<f64> {
        let start = night.and_hms_opt(21, 0, 0)?;
        let end = start + Duration::hours(9);
        self.hourly
            .iter()
            .filter(|h| h.at >= start && h.at <= end)
            .filter_map(|h| h.temp_c)
            .reduce(f64::min)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayOutlook {
    pub date: NaiveDate,
    pub radiation_kwh_m2: Option<f64>,
    pub sunshine_h: Option<f64>,
    pub cloud_pct: Option<f64>,
    pub rain_prob_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecentDay {
    pub date: NaiveDate,
    pub radiation_kwh_m2: Option<f64>,
    pub max_soc: Option<f64>,
    pub full_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourOutlook {
    pub at: NaiveDateTime,
    pub cloud_pct: Option<f64>,
    pub radiation_w_m2: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeatherSummary {
    pub next_day: Option<DayOutlook>,
    pub recent_avg_radiation_kwh_m2: Option<f64>,
    pub recent_days: Vec<RecentDay>,
    pub tonight_min_temp_c: Option<f64>,
    pub recent_nights_min_temp_c: Option<f64>,
    pub expected_pv_kwh_next_day: Option<f64>,
    pub hours_until_sunset: Vec<HourOutlook>,
}

pub struct SummaryInput<'a> {
    pub now: NaiveDateTime,
    /// The daylight date the battery will next be charged on.
    pub next_solar_day: NaiveDate,
    /// The night currently running, or the one coming up this evening.
    pub night: NaiveDate,
    pub sunset: NaiveDateTime,
    pub pv_array_watts: f64,
    pub solar_history: &'a [DaySolar],
}

pub fn summarize(forecast: &Forecast, input: SummaryInput<'_>) -> WeatherSummary {
    let next_day = forecast.day(input.next_solar_day).map(|day| DayOutlook {
        date: day.date,
        radiation_kwh_m2: day.radiation_kwh_m2,
        sunshine_h: day.sunshine_h,
        cloud_pct: forecast.daytime_cloud_avg(day.date),
        rain_prob_pct: day.rain_prob_pct,
    });

    let today = input.now.date();
    let recent_days: Vec<RecentDay> = forecast
        .daily
        .iter()
        .filter(|day| day.date < today && day.date >= today - Duration::days(7))
        .map(|day| {
            let solar = input.solar_history.iter().find(|s| s.date == day.date);
            RecentDay {
                date: day.date,
                radiation_kwh_m2: day.radiation_kwh_m2,
                max_soc: solar.and_then(|s| s.max_soc),
                full_at: solar.and_then(|s| s.full_at).map(|t| t.format("%H:%M").to_string()),
            }
        })
        .collect();
    let radiations: Vec<f64> = recent_days.iter().filter_map(|d| d.radiation_kwh_m2).collect();
    let recent_avg_radiation_kwh_m2 =
        (!radiations.is_empty()).then(|| radiations.iter().sum::<f64>() / radiations.len() as f64);

    let recent_night_mins: Vec<f64> = (1..=7)
        .filter_map(|back| forecast.night_min_temp(input.night - Duration::days(back)))
        .collect();
    let recent_nights_min_temp_c =
        (!recent_night_mins.is_empty()).then(|| recent_night_mins.iter().sum::<f64>() / recent_night_mins.len() as f64);

    let expected_pv_kwh_next_day = next_day
        .as_ref()
        .and_then(|day| day.radiation_kwh_m2)
        .filter(|_| input.pv_array_watts > 0.0)
        .map(|kwh_m2| kwh_m2 * input.pv_array_watts / 1000.0 * PERFORMANCE_RATIO);

    let hours_until_sunset = forecast
        .hours_between(input.now, input.sunset + Duration::hours(1))
        .into_iter()
        .map(|h| HourOutlook {
            at: h.at,
            cloud_pct: h.cloud_pct,
            radiation_w_m2: h.radiation_w_m2,
        })
        .collect();

    WeatherSummary {
        next_day,
        recent_avg_radiation_kwh_m2,
        recent_days,
        tonight_min_temp_c: forecast.night_min_temp(input.night),
        recent_nights_min_temp_c,
        expected_pv_kwh_next_day,
        hours_until_sunset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;
    use serde_json::json;

    fn fixture() -> Value {
        let mut times = Vec::new();
        let mut temps = Vec::new();
        let mut clouds = Vec::new();
        let mut radiation = Vec::new();
        for day in 19..=21 {
            for hour in 0..24 {
                times.push(format!("2026-09-{day:02}T{hour:02}:00"));
                temps.push(json!(if hour < 6 { 22.0 + day as f64 - 19.0 } else { 30.0 }));
                clouds.push(json!(if day == 21 { 90 } else { 10 }));
                let sun = if (7..=18).contains(&hour) { 600.0 } else { 0.0 };
                radiation.push(json!(sun));
            }
        }
        radiation[36] = Value::Null;
        json!({
            "hourly": { "time": times, "temperature_2m": temps, "cloud_cover": clouds, "shortwave_radiation": radiation },
            "daily": {
                "time": ["2026-09-19", "2026-09-20", "2026-09-21"],
                "shortwave_radiation_sum": [18.0, 21.6, 7.2],
                "sunshine_duration": [36000.0, 39600.0, 7200.0],
                "precipitation_probability_max": [0, 5, 80],
                "temperature_2m_min": [22.0, 23.0, 24.0]
            }
        })
    }

    fn at(day: u32, hour: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, day).unwrap().and_hms_opt(hour, 0, 0).unwrap()
    }

    #[test]
    fn parses_hourly_and_daily_with_nulls() {
        let forecast = parse(&fixture()).unwrap();
        assert_eq!(forecast.hourly.len(), 72);
        assert_eq!(forecast.hourly[36].radiation_w_m2, None);
        let day = forecast.day(at(20, 0).date()).unwrap();
        assert!((day.radiation_kwh_m2.unwrap() - 6.0).abs() < 1e-9);
        assert!((day.sunshine_h.unwrap() - 11.0).abs() < 1e-9);
    }

    #[test]
    fn radiation_during_uses_the_hour_ending_value() {
        let forecast = parse(&fixture()).unwrap();
        assert_eq!(forecast.radiation_during(at(20, 10) + Duration::minutes(20)), Some(600.0));
        assert_eq!(forecast.radiation_during(at(20, 22)), Some(0.0));
    }

    #[test]
    fn radiation_lookup_works_with_a_live_clock_that_has_fractional_seconds() {
        let forecast = parse(&fixture()).unwrap();
        let live = (at(20, 10) + Duration::minutes(20)).with_nanosecond(123_456_789).unwrap();
        assert_eq!(forecast.radiation_during(live), Some(600.0));
    }

    #[test]
    fn summary_compares_tomorrow_with_recent_days_and_joins_battery_history() {
        let forecast = parse(&fixture()).unwrap();
        let history = vec![DaySolar {
            date: at(20, 0).date(),
            max_soc: Some(100.0),
            full_at: NaiveTime::from_hms_opt(13, 10, 0),
            samples: 40,
        }];
        let summary = summarize(
            &forecast,
            SummaryInput {
                now: at(21, 1),
                next_solar_day: at(21, 0).date(),
                night: at(20, 0).date(),
                sunset: at(21, 18),
                pv_array_watts: 3000.0,
                solar_history: &history,
            },
        );
        let next = summary.next_day.unwrap();
        assert!((next.radiation_kwh_m2.unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(next.cloud_pct, Some(90.0));
        assert_eq!(summary.recent_days.len(), 2);
        assert_eq!(summary.recent_days[1].full_at.as_deref(), Some("13:10"));
        assert!((summary.recent_avg_radiation_kwh_m2.unwrap() - 5.5).abs() < 1e-9);
        assert!((summary.expected_pv_kwh_next_day.unwrap() - 4.5).abs() < 1e-9);
        assert_eq!(summary.tonight_min_temp_c, Some(24.0));
        assert_eq!(summary.recent_nights_min_temp_c, Some(22.5));
    }
}
