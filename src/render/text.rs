use std::fmt::Write;

use unicode_width::UnicodeWidthStr;

use crate::summary::{SPLIT_DISTANCE_M, STOP_MAX_SPEED_MPS, STOP_WINDOW_S, Split, Summary};

/// `detail` のときは日別の後ろに距離で区切った区間の表を足す
pub fn render(s: &Summary, detail: bool) -> String {
    let mut out = String::new();

    let stay = if s.nights == 0 {
        "日帰り".to_string()
    } else {
        format!("{}泊{}日", s.nights, s.nights + 1)
    };
    writeln!(out, "{}", s.name).unwrap();
    writeln!(
        out,
        "{} → {} ({})  {stay}",
        s.start.strftime("%Y-%m-%d %H:%M"),
        s.end.strftime("%Y-%m-%d %H:%M"),
        s.start.strftime("%Z"),
    )
    .unwrap();
    out.push('\n');

    let trip = if s.is_round_trip() {
        "往復またはループ"
    } else {
        "片道"
    };
    // 最高点と最低点の行で、標高の後ろの日時と座標の位置を揃える
    let ele_width = meters(s.max.ele_m).width().max(meters(s.min.ele_m).width());
    let extreme = |e: &crate::summary::Extreme| {
        format!(
            "{}   {}  ({:.5}, {:.5})",
            pad_left(&meters(e.ele_m), ele_width),
            e.time.strftime("%m-%d %H:%M"),
            e.lat,
            e.lon
        )
    };
    let rows = [
        ("距離", km(s.distance_m)),
        (
            "累積標高",
            format!("↑ {}   ↓ {}", meters(s.gain_m), meters(s.loss_m)),
        ),
        ("最高点", extreme(&s.max)),
        ("最低点", extreme(&s.min)),
        ("経過時間", hours_minutes(s.elapsed_s)),
        (
            "行動時間",
            format!(
                "{}   ※停止判定: {}分間の平均速度が {} m/s 未満",
                hours_minutes(s.moving_s),
                STOP_WINDOW_S / 60,
                STOP_MAX_SPEED_MPS
            ),
        ),
        (
            "起点と終点",
            format!("{} 離れている（{trip}）", meters(s.start_end_distance_m)),
        ),
    ];
    let label_width = rows.iter().map(|(l, _)| l.width()).max().unwrap_or(0);
    for (label, value) in rows {
        writeln!(out, "  {}    {value}", pad_right(label, label_width)).unwrap();
    }

    out.push_str("\n日別\n");
    let mut days =
        vec![["日付", "開始", "終了", "点数", "距離", "↑", "↓", "最高"].map(String::from)];
    for d in &s.days {
        days.push([
            d.date.strftime("%m-%d").to_string(),
            d.start.strftime("%H:%M").to_string(),
            d.end.strftime("%H:%M").to_string(),
            d.points.to_string(),
            km(d.distance_m),
            meters(d.gain_m),
            meters(d.loss_m),
            meters(d.max_ele_m),
        ]);
    }
    // 日付・開始・終了は左寄せ
    write_table(&mut out, &days, 3);

    if detail {
        writeln!(
            out,
            "\n区間（{} km ごと。ペースと速度は行動時間から計算）",
            SPLIT_DISTANCE_M / 1000.0
        )
        .unwrap();
        let mut splits = vec![
            [
                "区間",
                "開始",
                "距離",
                "行動時間",
                "ペース",
                "速度",
                "↑",
                "↓",
            ]
            .map(String::from),
        ];
        for (i, sp) in s.splits.iter().enumerate() {
            splits.push([
                (i + 1).to_string(),
                sp.start.strftime("%m-%d %H:%M").to_string(),
                km(sp.distance_m),
                hours_minutes(sp.moving_s),
                pace(sp),
                speed(sp),
                meters(sp.gain_m),
                meters(sp.loss_m),
            ]);
        }
        write_table(&mut out, &splits, 0);
    }

    out
}

/// 先頭の `left_aligned` 列は左寄せ、残りの数値の列は右寄せにする
fn write_table<const N: usize>(out: &mut String, rows: &[[String; N]], left_aligned: usize) {
    let widths: Vec<usize> = (0..N)
        .map(|i| rows.iter().map(|row| row[i].width()).max().unwrap_or(0))
        .collect();
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .zip(&widths)
            .enumerate()
            .map(|(i, (cell, &w))| {
                if i < left_aligned {
                    pad_right(cell, w)
                } else {
                    pad_left(cell, w)
                }
            })
            .collect();
        writeln!(out, "  {}", cells.join("  ").trim_end()).unwrap();
    }
}

/// 1 km あたりの時間。行動時間が無い区間は "-"
fn pace(sp: &Split) -> String {
    match sp.speed_mps() {
        Some(mps) if sp.distance_m > 0.0 => {
            let secs = (1000.0 / mps).round() as i64;
            format!("{}'{:02}\"/km", secs / 60, secs % 60)
        }
        _ => "-".to_string(),
    }
}

fn speed(sp: &Split) -> String {
    match sp.speed_mps() {
        Some(mps) => format!("{:.1} km/h", mps * 3.6),
        None => "-".to_string(),
    }
}

fn km(m: f64) -> String {
    format!("{:.1} km", m / 1000.0)
}

fn meters(m: f64) -> String {
    format!("{} m", with_commas(m.round() as i64))
}

fn with_commas(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// 時間は 24 で繰り上げない
fn hours_minutes(secs: i64) -> String {
    format!("{}h {}m", secs / 3600, secs % 3600 / 60)
}

// format! の幅指定は文字数で数えるので、全角を 2 として数える表示幅で揃える
fn pad_right(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(s.width())))
}

fn pad_left(s: &str, width: usize) -> String {
    format!("{}{s}", " ".repeat(width.saturating_sub(s.width())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers() {
        assert_eq!(with_commas(0), "0");
        assert_eq!(with_commas(999), "999");
        assert_eq!(with_commas(1000), "1,000");
        assert_eq!(with_commas(1234567), "1,234,567");
        assert_eq!(with_commas(-1234), "-1,234");
        assert_eq!(meters(2830.5), "2,831 m");
        assert_eq!(km(46310.4), "46.3 km");
        assert_eq!(hours_minutes(4_709_780), "1308h 16m");
    }

    #[test]
    fn pads_by_display_width() {
        assert_eq!(pad_right("距離", 10), "距離      ");
        assert_eq!(pad_right("起点と終点", 10), "起点と終点");
        assert_eq!(pad_left("1 m", 5), "  1 m");
    }

    #[test]
    fn snapshot() {
        insta::assert_snapshot!(render(&crate::render::tests::sample_summary(), false));
    }

    #[test]
    fn snapshot_day_trip() {
        insta::assert_snapshot!(render(&crate::render::tests::day_trip_summary(), false));
    }

    #[test]
    fn snapshot_detail() {
        insta::assert_snapshot!(render(&crate::render::tests::day_trip_summary(), true));
    }
}
