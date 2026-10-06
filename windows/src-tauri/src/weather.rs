// The Hub's Weather: the current conditions and the next days for a city, from Open-Meteo
// (no account, no key). It is a network call, so it is off until the user turns it on and
// names a city (Settings → Hub); nothing is asked while the Weather tab is closed, and an
// answer is kept for 15 minutes.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

const TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub date: String,
    pub min: f64,
    pub max: f64,
    /// Highest chance of rain that day, percent.
    pub rain: u32,
    pub code: u32,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub city: String,
    pub temp: f64,
    pub feels: f64,
    pub humidity: u32,
    pub wind_kmh: f64,
    /// WMO weather code (the page turns it into words and a symbol).
    pub code: u32,
    /// Highest chance of rain in the next six hours, percent.
    pub rain_next_6h: u32,
    pub days: Vec<Day>,
}

static CACHE: Mutex<Option<HashMap<String, (Instant, Weather)>>> = Mutex::new(None);
static PLACES: Mutex<Option<HashMap<String, (f64, f64, String)>>> = Mutex::new(None);

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

/// (latitude, longitude, display name) of a city, remembered for the run.
async fn place(city: &str) -> Result<(f64, f64, String), String> {
    let key = city.trim().to_lowercase();
    if let Some(p) = PLACES.lock().unwrap().as_ref().and_then(|m| m.get(&key)).cloned() {
        return Ok(p);
    }
    let resp = client()?
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[("name", city.trim()), ("count", "1"), ("language", "en")])
        .send()
        .await
        .map_err(|e| format!("Cannot reach open-meteo.com: {e}"))?;
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    let r = v["results"].get(0).ok_or_else(|| format!("No place called “{}”", city.trim()))?;
    let found = (
        r["latitude"].as_f64().ok_or("bad place")?,
        r["longitude"].as_f64().ok_or("bad place")?,
        match (r["name"].as_str(), r["admin1"].as_str()) {
            (Some(n), Some(a)) if a != n => format!("{n}, {a}"),
            (Some(n), _) => n.to_string(),
            _ => city.trim().to_string(),
        },
    );
    PLACES.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, found.clone());
    Ok(found)
}

/// The conditions in an Open-Meteo forecast answer.
pub fn parse_forecast(v: &Value, name: &str) -> Result<Weather, String> {
    let cur = &v["current"];
    let temp = cur["temperature_2m"].as_f64().ok_or("The forecast had no current temperature")?;
    // Rain: the highest hourly chance from the current hour for six hours.
    let now = cur["time"].as_str().unwrap_or("");
    let times = v["hourly"]["time"].as_array().cloned().unwrap_or_default();
    let probs = v["hourly"]["precipitation_probability"].as_array().cloned().unwrap_or_default();
    let start = times.iter().position(|t| t.as_str().unwrap_or("") >= now).unwrap_or(0);
    let rain_next_6h = probs.iter().skip(start).take(6).filter_map(Value::as_f64).fold(0.0, f64::max) as u32;
    let d = &v["daily"];
    let dates = d["time"].as_array().cloned().unwrap_or_default();
    let days = dates
        .iter()
        .enumerate()
        .filter_map(|(i, date)| {
            Some(Day {
                date: date.as_str()?.to_string(),
                min: d["temperature_2m_min"].get(i)?.as_f64()?,
                max: d["temperature_2m_max"].get(i)?.as_f64()?,
                rain: d["precipitation_probability_max"].get(i).and_then(Value::as_f64).unwrap_or(0.0) as u32,
                code: d["weather_code"].get(i).and_then(Value::as_u64).unwrap_or(0) as u32,
            })
        })
        .collect();
    Ok(Weather {
        city: name.to_string(),
        temp,
        feels: cur["apparent_temperature"].as_f64().unwrap_or(temp),
        humidity: cur["relative_humidity_2m"].as_f64().unwrap_or(0.0) as u32,
        wind_kmh: cur["wind_speed_10m"].as_f64().unwrap_or(0.0),
        code: cur["weather_code"].as_u64().unwrap_or(0) as u32,
        rain_next_6h,
        days,
    })
}

pub async fn fetch(city: &str) -> Result<Weather, String> {
    let key = city.trim().to_lowercase();
    if let Some((at, w)) = CACHE.lock().unwrap().as_ref().and_then(|m| m.get(&key)).cloned() {
        if at.elapsed() < TTL {
            return Ok(w);
        }
    }
    let (lat, lon, name) = place(city).await?;
    let resp = client()?
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("current", "temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m".into()),
            ("hourly", "precipitation_probability".into()),
            ("daily", "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max".into()),
            ("timezone", "auto".into()),
            ("forecast_days", "4".into()),
        ])
        .send()
        .await
        .map_err(|e| format!("Cannot reach open-meteo.com: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("open-meteo.com answered {}", resp.status().as_u16()));
    }
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    let w = parse_forecast(&v, &name)?;
    CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(key, (Instant::now(), w.clone()));
    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forecast_answer_becomes_the_conditions_the_chance_of_rain_and_the_days() {
        let v: Value = serde_json::from_str(
            r#"{"current":{"time":"2026-10-05T20:15","temperature_2m":23.4,"relative_humidity_2m":55,"apparent_temperature":24.1,"weather_code":3,"wind_speed_10m":9.4},
                "hourly":{"time":["2026-10-05T19:00","2026-10-05T20:00","2026-10-05T21:00","2026-10-05T22:00","2026-10-05T23:00","2026-10-06T00:00","2026-10-06T01:00","2026-10-06T02:00"],
                          "precipitation_probability":[90,10,20,70,30,5,0,100]},
                "daily":{"time":["2026-10-05","2026-10-06"],"weather_code":[3,61],"temperature_2m_max":[27.0,25.5],"temperature_2m_min":[15.2,16.0],"precipitation_probability_max":[70,100]}}"#,
        )
        .unwrap();
        let w = parse_forecast(&v, "Zapopan, Jalisco").unwrap();
        assert_eq!(w.city, "Zapopan, Jalisco");
        assert_eq!((w.temp, w.feels, w.humidity, w.code), (23.4, 24.1, 55, 3));
        assert_eq!(w.rain_next_6h, 100, "six hours from 21:00 (the first hour not before 20:15): 20, 70, 30, 5, 0, 100");
        assert_eq!(w.days.len(), 2);
        assert_eq!(w.days[1], Day { date: "2026-10-06".into(), min: 16.0, max: 25.5, rain: 100, code: 61 });
    }

    #[test]
    fn an_answer_without_a_temperature_is_an_error_not_a_zero() {
        assert!(parse_forecast(&serde_json::json!({"current":{}}), "x").is_err());
    }
}
