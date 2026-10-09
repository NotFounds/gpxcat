use jiff::Zoned;
use serde::Serialize;

use crate::summary::{Extreme, STOP_MAX_SPEED_MPS, STOP_WINDOW_S, Split, Summary};

#[derive(Serialize)]
struct Output<'a> {
    name: &'a str,
    timezone: &'a str,
    start: String,
    end: String,
    nights: i32,
    points: usize,
    distance_m: f64,
    elevation: Elevation,
    duration: Duration,
    start_end_distance_m: f64,
    days: Vec<Day>,
    /// --detail のときだけ出す
    #[serde(skip_serializing_if = "Option::is_none")]
    splits: Option<Vec<SplitOutput>>,
}

#[derive(Serialize)]
struct Elevation {
    gain_m: f64,
    loss_m: f64,
    max: Point,
    min: Point,
}

#[derive(Serialize)]
struct Point {
    ele_m: f64,
    time: String,
    lat: f64,
    lon: f64,
}

#[derive(Serialize)]
struct Duration {
    elapsed_s: i64,
    moving_s: i64,
    stop_rule: StopRule,
}

#[derive(Serialize)]
struct StopRule {
    window_s: i64,
    max_speed_mps: f64,
}

#[derive(Serialize)]
struct Day {
    date: String,
    start: String,
    end: String,
    points: usize,
    distance_m: f64,
    gain_m: f64,
    loss_m: f64,
    max_ele_m: f64,
}

#[derive(Serialize)]
struct SplitOutput {
    start: String,
    end: String,
    distance_m: f64,
    elapsed_s: i64,
    moving_s: i64,
    /// 行動時間から計算する。行動時間が 0 のときは null
    speed_mps: Option<f64>,
    pace_s_per_km: Option<f64>,
    gain_m: f64,
    loss_m: f64,
}

/// `detail` のときは距離で区切った区間を `splits` として足す
pub fn render(s: &Summary, detail: bool) -> String {
    let output = Output {
        name: &s.name,
        timezone: s.timezone.iana_name().unwrap_or_default(),
        start: iso8601(&s.start),
        end: iso8601(&s.end),
        nights: s.nights,
        points: s.points,
        distance_m: s.distance_m,
        elevation: Elevation {
            gain_m: s.gain_m,
            loss_m: s.loss_m,
            max: point(&s.max),
            min: point(&s.min),
        },
        duration: Duration {
            elapsed_s: s.elapsed_s,
            moving_s: s.moving_s,
            stop_rule: StopRule {
                window_s: STOP_WINDOW_S,
                max_speed_mps: STOP_MAX_SPEED_MPS,
            },
        },
        start_end_distance_m: s.start_end_distance_m,
        days: s
            .days
            .iter()
            .map(|d| Day {
                date: d.date.to_string(),
                start: d.start.strftime("%H:%M:%S").to_string(),
                end: d.end.strftime("%H:%M:%S").to_string(),
                points: d.points,
                distance_m: d.distance_m,
                gain_m: d.gain_m,
                loss_m: d.loss_m,
                max_ele_m: d.max_ele_m,
            })
            .collect(),
        splits: detail.then(|| s.splits.iter().map(split).collect()),
    };
    let mut json = serde_json::to_string_pretty(&output).expect("数値と文字列だけなので失敗しない");
    json.push('\n');
    json
}

fn point(e: &Extreme) -> Point {
    Point {
        ele_m: e.ele_m,
        time: iso8601(&e.time),
        lat: e.lat,
        lon: e.lon,
    }
}

fn split(sp: &Split) -> SplitOutput {
    SplitOutput {
        start: iso8601(&sp.start),
        end: iso8601(&sp.end),
        distance_m: sp.distance_m,
        elapsed_s: sp.elapsed_s,
        moving_s: sp.moving_s,
        speed_mps: sp.speed_mps(),
        pace_s_per_km: sp
            .speed_mps()
            .filter(|&mps| mps > 0.0)
            .map(|mps| 1000.0 / mps),
        gain_m: sp.gain_m,
        loss_m: sp.loss_m,
    }
}

fn iso8601(z: &Zoned) -> String {
    z.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot() {
        insta::assert_snapshot!(render(&crate::render::tests::sample_summary(), false));
    }

    #[test]
    fn snapshot_detail() {
        insta::assert_snapshot!(render(&crate::render::tests::day_trip_summary(), true));
    }
}
