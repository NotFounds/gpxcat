use std::collections::BTreeMap;

use jiff::Zoned;
use jiff::civil::{Date, Time};
use jiff::tz::TimeZone;

use crate::gpx::{Point, Track};

/// 停止の判定に使う時間の幅。各点からこの秒数以上後の最初の点までを 1 つの幅とする
pub const STOP_WINDOW_S: i64 = 120;
/// 幅の両端の直線距離 ÷ 時間差がこの速度未満なら、その幅を停止とみなす。
/// 2 分で 24 m しか進まない速さで、活動の種類を問わず「ほぼその場から動いていない」ことを表す
pub const STOP_MAX_SPEED_MPS: f64 = 0.2;
/// --detail で出す区間の長さ。区間の距離がこれ以上になった最初の点で区切る
pub const SPLIT_DISTANCE_M: f64 = 1000.0;
/// 起点と終点の距離がこれ以下なら往復またはループとみなす
pub const ROUND_TRIP_MAX_DISTANCE_M: f64 = 200.0;

const EARTH_RADIUS_M: f64 = 6_371_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub name: String,
    pub timezone: TimeZone,
    pub start: Zoned,
    pub end: Zoned,
    pub nights: i32,
    pub points: usize,
    pub distance_m: f64,
    pub gain_m: f64,
    pub loss_m: f64,
    pub max: Extreme,
    pub min: Extreme,
    pub elapsed_s: i64,
    pub moving_s: i64,
    pub start_end_distance_m: f64,
    pub days: Vec<Day>,
    pub splits: Vec<Split>,
}

/// 最高点・最低点
#[derive(Debug, Clone, PartialEq)]
pub struct Extreme {
    pub ele_m: f64,
    pub time: Zoned,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    pub date: Date,
    pub start: Time,
    pub end: Time,
    pub points: usize,
    pub distance_m: f64,
    pub gain_m: f64,
    pub loss_m: f64,
    pub max_ele_m: f64,
}

/// 距離で区切った区間。前の区間の最後の点から、距離が SPLIT_DISTANCE_M 以上になった最初の点まで
#[derive(Debug, Clone, PartialEq)]
pub struct Split {
    pub start: Zoned,
    pub end: Zoned,
    pub distance_m: f64,
    pub elapsed_s: i64,
    pub moving_s: i64,
    pub gain_m: f64,
    pub loss_m: f64,
}

impl Split {
    /// 行動時間で割った速度。行動時間が 0 のときは None
    pub fn speed_mps(&self) -> Option<f64> {
        (self.moving_s > 0).then(|| self.distance_m / self.moving_s as f64)
    }
}

impl Summary {
    pub fn is_round_trip(&self) -> bool {
        self.start_end_distance_m <= ROUND_TRIP_MAX_DISTANCE_M
    }
}

pub fn summarize(track: &Track, tz: &TimeZone) -> Result<Summary, String> {
    let points = &track.points;
    if points.len() < 2 {
        return Err("点が 2 つ未満のため集計できません".to_string());
    }
    let (first, last) = (&points[0], &points[points.len() - 1]);

    let mut distance_m = 0.0;
    let mut gain_m = 0.0;
    let mut loss_m = 0.0;
    for w in points.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        distance_m += haversine_m(a, b);
        let (gain, loss) = climb(a, b);
        gain_m += gain;
        loss_m += loss;
    }

    let start = first.time.to_zoned(tz.clone());
    let end = last.time.to_zoned(tz.clone());
    let elapsed_s = interval_s(first, last);
    let stops = stops(points);

    Ok(Summary {
        name: track.name.clone(),
        timezone: tz.clone(),
        nights: (end.date() - start.date()).get_days(),
        start,
        end,
        points: points.len(),
        distance_m,
        gain_m,
        loss_m,
        // max_by は同じ値なら後ろを採るので、先に出てくる点を採れるよう比較で書く
        max: extreme(points, tz, |candidate, best| candidate > best),
        min: extreme(points, tz, |candidate, best| candidate < best),
        elapsed_s,
        moving_s: elapsed_s - stops.iter().map(|(s, e)| e - s).sum::<i64>(),
        start_end_distance_m: haversine_m(first, last),
        days: days(points, tz),
        splits: splits(points, &stops, tz),
    })
}

fn extreme(points: &[Point], tz: &TimeZone, better: impl Fn(f64, f64) -> bool) -> Extreme {
    let mut best = &points[0];
    for p in &points[1..] {
        if better(p.ele, best.ele) {
            best = p;
        }
    }
    Extreme {
        ele_m: best.ele,
        time: best.time.to_zoned(tz.clone()),
        lat: best.lat,
        lon: best.lon,
    }
}

/// 両端の点が同じ日に属する区間だけを、その日の距離・累積標高に入れる
fn days(points: &[Point], tz: &TimeZone) -> Vec<Day> {
    let date_of = |p: &Point| p.time.to_zoned(tz.clone()).date();

    let mut days: BTreeMap<Date, Day> = BTreeMap::new();
    for p in points {
        let zoned = p.time.to_zoned(tz.clone());
        let day = days.entry(zoned.date()).or_insert_with(|| Day {
            date: zoned.date(),
            start: zoned.time(),
            end: zoned.time(),
            points: 0,
            distance_m: 0.0,
            gain_m: 0.0,
            loss_m: 0.0,
            max_ele_m: p.ele,
        });
        day.start = day.start.min(zoned.time());
        day.end = day.end.max(zoned.time());
        day.points += 1;
        if p.ele > day.max_ele_m {
            day.max_ele_m = p.ele;
        }
    }

    for w in points.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        let date = date_of(a);
        if date != date_of(b) {
            continue;
        }
        let day = days.get_mut(&date).expect("点のある日はすべて登録済み");
        day.distance_m += haversine_m(a, b);
        let (gain, loss) = climb(a, b);
        day.gain_m += gain;
        day.loss_m += loss;
    }

    days.into_values().collect()
}

fn splits(points: &[Point], stops: &[(i64, i64)], tz: &TimeZone) -> Vec<Split> {
    let mut splits = Vec::new();
    let mut start = &points[0];
    let (mut distance_m, mut gain_m, mut loss_m) = (0.0, 0.0, 0.0);
    for (i, w) in points.windows(2).enumerate() {
        let (a, b) = (&w[0], &w[1]);
        distance_m += haversine_m(a, b);
        let (gain, loss) = climb(a, b);
        gain_m += gain;
        loss_m += loss;
        let is_last = i + 2 == points.len();
        if distance_m < SPLIT_DISTANCE_M && !is_last {
            continue;
        }
        let elapsed_s = interval_s(start, b);
        let (from, to) = (start.time.as_second(), b.time.as_second());
        let stopped_s: i64 = stops
            .iter()
            .map(|&(s, e)| (e.min(to) - s.max(from)).max(0))
            .sum();
        splits.push(Split {
            start: start.time.to_zoned(tz.clone()),
            end: b.time.to_zoned(tz.clone()),
            distance_m,
            elapsed_s,
            moving_s: elapsed_s - stopped_s,
            gain_m,
            loss_m,
        });
        start = b;
        (distance_m, gain_m, loss_m) = (0.0, 0.0, 0.0);
    }
    splits
}

pub fn haversine_m(a: &Point, b: &Point) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// 区間の (登り, 下り)。どちらも 0 以上
fn climb(a: &Point, b: &Point) -> (f64, f64) {
    let diff = b.ele - a.ele;
    if diff > 0.0 {
        (diff, 0.0)
    } else {
        (0.0, -diff)
    }
}

fn interval_s(a: &Point, b: &Point) -> i64 {
    b.time.as_second() - a.time.as_second()
}

/// 停止とみなした幅を、重なりをまとめた (開始, 終了) の秒の列。
/// 隣り合う点ではなく一定の時間の幅で判定するので、記録の間隔によらず同じように停止を検出でき、
/// GPS の位置のぶれも幅の分だけ平均される
fn stops(points: &[Point]) -> Vec<(i64, i64)> {
    let mut stops = Vec::new();
    // まとめている途中の停止の (開始, 終了)
    let mut current: Option<(i64, i64)> = None;
    let mut j = 0;
    for (i, a) in points.iter().enumerate() {
        j = j.max(i + 1);
        while j < points.len() && interval_s(a, &points[j]) < STOP_WINDOW_S {
            j += 1;
        }
        let Some(b) = points.get(j) else { break };
        if haversine_m(a, b) / (interval_s(a, b) as f64) >= STOP_MAX_SPEED_MPS {
            continue;
        }
        let (start, end) = (a.time.as_second(), b.time.as_second());
        current = match current {
            Some((s, e)) if start <= e => Some((s, e.max(end))),
            Some(stop) => {
                stops.push(stop);
                Some((start, end))
            }
            None => Some((start, end)),
        };
    }
    stops.extend(current);
    stops
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::Timestamp;

    fn tokyo() -> TimeZone {
        TimeZone::get("Asia/Tokyo").unwrap()
    }

    fn point(lat: f64, lon: f64, ele: f64, time: &str) -> Point {
        Point {
            lat,
            lon,
            ele,
            time: time.parse::<Timestamp>().unwrap(),
        }
    }

    fn track(points: Vec<Point>) -> Track {
        Track {
            name: "test".to_string(),
            points,
        }
    }

    /// 赤道上で経度方向に `m` メートル進んだ点の経度
    fn lon_for_m(m: f64) -> f64 {
        (m / EARTH_RADIUS_M).to_degrees()
    }

    #[test]
    fn haversine_matches_known_distances() {
        let t = "2026-01-01T00:00:00Z";
        // 子午線に沿った緯度 1 度 = 2πR / 360
        let d = haversine_m(&point(0.0, 0.0, 0.0, t), &point(1.0, 0.0, 0.0, t));
        assert!((d - 111_194.926_6).abs() < 0.001, "{d}");
        // 北緯 60 度では経度 1 度が赤道上の約半分になる
        let d = haversine_m(&point(60.0, 0.0, 0.0, t), &point(60.0, 1.0, 0.0, t));
        assert!((d - 55_597.46).abs() < 1.0, "{d}");
        assert_eq!(
            haversine_m(&point(35.0, 139.0, 0.0, t), &point(35.0, 139.0, 0.0, t)),
            0.0
        );
    }

    #[test]
    fn totals_distance_climb_and_extremes() {
        let s = summarize(
            &track(vec![
                point(0.0, 0.0, 100.0, "2026-08-15T00:00:00Z"),
                point(0.0, lon_for_m(100.0), 150.0, "2026-08-15T00:01:00Z"),
                point(0.0, lon_for_m(200.0), 120.0, "2026-08-15T00:02:00Z"),
                point(0.0, lon_for_m(300.0), 150.0, "2026-08-15T00:03:00Z"),
                point(0.0, lon_for_m(400.0), 100.0, "2026-08-15T00:04:00Z"),
            ]),
            &tokyo(),
        )
        .unwrap();
        assert_eq!(s.points, 5);
        assert!((s.distance_m - 400.0).abs() < 1e-6, "{}", s.distance_m);
        assert_eq!(s.gain_m, 80.0);
        assert_eq!(s.loss_m, 80.0);
        // 同じ標高の点が複数あるときは先に出てくる点
        assert_eq!(s.max.ele_m, 150.0);
        assert_eq!(s.max.lon, lon_for_m(100.0));
        assert_eq!(s.min.ele_m, 100.0);
        assert_eq!(s.min.lon, 0.0);
        assert_eq!(s.elapsed_s, 240);
        assert_eq!(s.moving_s, 240);
        assert_eq!(s.nights, 0);
        assert!((s.start_end_distance_m - 400.0).abs() < 1e-6);
        assert!(!s.is_round_trip());
        assert_eq!(s.start.to_string(), "2026-08-15T09:00:00+09:00[Asia/Tokyo]");
    }

    fn moving_s(dt_s: i64, distance_m: f64) -> i64 {
        let end = Timestamp::from_second(1_800_000_000 + dt_s).unwrap();
        let s = summarize(
            &track(vec![
                Point {
                    lat: 0.0,
                    lon: 0.0,
                    ele: 0.0,
                    time: Timestamp::from_second(1_800_000_000).unwrap(),
                },
                Point {
                    lat: 0.0,
                    lon: lon_for_m(distance_m),
                    ele: 0.0,
                    time: end,
                },
            ]),
            &tokyo(),
        )
        .unwrap();
        s.moving_s
    }

    #[test]
    fn stop_needs_window_of_at_least_120_seconds() {
        // 119 秒は速度が 0 でも停止にしない
        assert_eq!(moving_s(119, 0.0), 119);
        assert_eq!(moving_s(120, 0.0), 0);
    }

    #[test]
    fn stop_needs_speed_below_0_2_mps() {
        // 1000 秒で 200 m はちょうど 0.2 m/s なので停止にしない
        assert_eq!(moving_s(1000, 200.0), 1000);
        assert_eq!(moving_s(1000, 199.0), 0);
    }

    /// (秒数, 速度 m/s) の区間を順につなぎ、10 秒ごとに記録した点列
    fn every_10_seconds(speeds: &[(i64, f64)]) -> Track {
        let mut points = Vec::new();
        let (mut t, mut x) = (1_800_000_000, 0.0);
        for &(seconds, speed) in speeds {
            for _ in 0..seconds / 10 {
                points.push(Point {
                    lat: 0.0,
                    lon: lon_for_m(x),
                    ele: 0.0,
                    time: Timestamp::from_second(t).unwrap(),
                });
                t += 10;
                x += speed * 10.0;
            }
        }
        points.push(Point {
            lat: 0.0,
            lon: lon_for_m(x),
            ele: 0.0,
            time: Timestamp::from_second(t).unwrap(),
        });
        track(points)
    }

    #[test]
    fn detects_stop_in_densely_recorded_points() {
        // 1 m/s で 300 秒 → 300 秒停止 → 1 m/s で 300 秒
        let s = summarize(
            &every_10_seconds(&[(300, 1.0), (300, 0.0), (300, 1.0)]),
            &tokyo(),
        )
        .unwrap();
        // 幅の中の移動が 24 m 未満なら停止になるので、停止の前後 20 秒も停止に含まれる。
        // 停止は 280 秒 〜 620 秒の 340 秒
        assert_eq!(s.elapsed_s, 900);
        assert_eq!(s.moving_s, 560);
    }

    #[test]
    fn separate_stops_are_summed() {
        let s = summarize(
            &every_10_seconds(&[(300, 1.0), (200, 0.0), (300, 1.0), (200, 0.0), (300, 1.0)]),
            &tokyo(),
        )
        .unwrap();
        // それぞれの停止が前後 20 秒ずつ広がって 240 秒になる
        assert_eq!(s.moving_s, 1300 - 240 * 2);
    }

    #[test]
    fn splits_every_kilometer_with_moving_time() {
        // 1.1 m/s で 1200 秒 → 300 秒停止 → 1.1 m/s で 900 秒。
        // 10 秒で 11 m 進むので、区間はちょうど 1 km ではなく 1,001 m で区切れる
        let s = summarize(
            &every_10_seconds(&[(1200, 1.1), (300, 0.0), (900, 1.1)]),
            &tokyo(),
        )
        .unwrap();
        let rows: Vec<(f64, i64, i64)> = s
            .splits
            .iter()
            .map(|sp| {
                (
                    (sp.distance_m * 1000.0).round() / 1000.0,
                    sp.elapsed_s,
                    sp.moving_s,
                )
            })
            .collect();
        // 停止は 1180 秒 〜 1520 秒の 340 秒で、2 区間目 (910 秒 〜 2120 秒) に入る
        assert_eq!(
            rows,
            vec![(1001.0, 910, 910), (1001.0, 1210, 870), (308.0, 280, 280)]
        );
        assert!((s.splits[0].speed_mps().unwrap() - 1.1).abs() < 1e-9);
    }

    #[test]
    fn split_without_moving_time_has_no_speed() {
        let s = summarize(&every_10_seconds(&[(300, 0.0)]), &tokyo()).unwrap();
        assert_eq!(s.splits.len(), 1);
        assert_eq!(s.splits[0].moving_s, 0);
        assert_eq!(s.splits[0].speed_mps(), None);
    }

    #[test]
    fn slow_but_steady_movement_is_not_a_stop() {
        // 0.3 m/s は急な登りの水平方向の速さ程度
        let s = summarize(&every_10_seconds(&[(600, 0.3)]), &tokyo()).unwrap();
        assert_eq!(s.moving_s, 600);
    }

    #[test]
    fn segment_crossing_midnight_belongs_to_no_day() {
        // JST の 23:59 → 翌 00:01
        let s = summarize(
            &track(vec![
                point(0.0, 0.0, 100.0, "2026-08-15T14:58:00Z"),
                point(0.0, lon_for_m(50.0), 110.0, "2026-08-15T14:59:00Z"),
                point(0.0, lon_for_m(150.0), 130.0, "2026-08-15T15:01:00Z"),
                point(0.0, lon_for_m(170.0), 125.0, "2026-08-15T15:02:00Z"),
            ]),
            &tokyo(),
        )
        .unwrap();
        assert_eq!(s.nights, 1);
        assert!((s.distance_m - 170.0).abs() < 1e-6);
        assert_eq!(s.days.len(), 2);

        let d1 = &s.days[0];
        assert_eq!(d1.date.to_string(), "2026-08-15");
        assert_eq!(d1.start.to_string(), "23:58:00");
        assert_eq!(d1.end.to_string(), "23:59:00");
        assert_eq!(d1.points, 2);
        assert!((d1.distance_m - 50.0).abs() < 1e-6);
        assert_eq!((d1.gain_m, d1.loss_m, d1.max_ele_m), (10.0, 0.0, 110.0));

        let d2 = &s.days[1];
        assert_eq!(d2.date.to_string(), "2026-08-16");
        assert_eq!(d2.points, 2);
        assert!((d2.distance_m - 20.0).abs() < 1e-6);
        assert_eq!((d2.gain_m, d2.loss_m, d2.max_ele_m), (0.0, 5.0, 130.0));
    }

    #[test]
    fn days_without_points_are_skipped() {
        let s = summarize(
            &track(vec![
                point(0.0, 0.0, 0.0, "2026-08-15T00:00:00Z"),
                point(0.0, 0.0, 0.0, "2026-08-18T00:00:00Z"),
            ]),
            &tokyo(),
        )
        .unwrap();
        assert_eq!(s.nights, 3);
        assert_eq!(s.days.len(), 2);
        assert!(s.is_round_trip());
    }

    #[test]
    fn rejects_fewer_than_two_points() {
        let one = track(vec![point(0.0, 0.0, 0.0, "2026-08-15T00:00:00Z")]);
        assert!(summarize(&one, &tokyo()).is_err());
        assert!(summarize(&track(vec![]), &tokyo()).is_err());
    }
}
