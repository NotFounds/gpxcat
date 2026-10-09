use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
    pub ele: f64,
    pub time: Timestamp,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub name: String,
    pub points: Vec<Point>,
}

/// 読み途中の trkpt。必須の値が揃っているかは閉じタグで確かめる
#[derive(Default)]
struct PartialPoint {
    lat: Option<String>,
    lon: Option<String>,
    ele: Option<String>,
    time: Option<String>,
}

/// trk > trkseg > trkpt をファイル順に 1 本の点列として読む。
/// 最初の trk/name が無いときは `default_name` を名前にする
pub fn parse(xml: &str, default_name: &str) -> Result<Track, String> {
    let mut reader = Reader::from_str(xml);

    let mut path: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut trk_count = 0;
    let mut name: Option<String> = None;
    let mut current: Option<PartialPoint> = None;
    let mut points = Vec::new();

    loop {
        let event = reader.read_event().map_err(|e| {
            format!(
                "XML を解析できません（{} バイト目付近）: {e}",
                reader.buffer_position()
            )
        })?;
        match event {
            Event::Start(e) => {
                let local = local_name(&e);
                // gpx 直下の trk だけを数える
                if local == "trk" && path.len() <= 1 {
                    trk_count += 1;
                }
                if local == "trkpt" && ends_with(&path, &["trk", "trkseg"]) {
                    current = Some(start_point(&e)?);
                }
                path.push(local);
                text.clear();
            }
            Event::Empty(e) => {
                // <trkpt lat=".." lon=".."/> は ele と time が無いのでエラーになる
                if local_name(&e) == "trkpt" && ends_with(&path, &["trk", "trkseg"]) {
                    points.push(finish_point(start_point(&e)?, points.len() + 1)?);
                }
            }
            Event::Text(t) => text.push_str(&t.xml10_content()),
            Event::CData(t) => text.push_str(&t),
            Event::GeneralRef(r) => {
                if let Some(c) = r.resolve_char_ref().map_err(|e| e.to_string())? {
                    text.push(c);
                } else if let Some(s) = resolve_xml_entity(&r) {
                    text.push_str(s);
                }
            }
            Event::End(_) => {
                if ends_with(&path, &["trk", "name"]) && trk_count == 1 && name.is_none() {
                    name = Some(text.trim().to_string());
                }
                if let Some(p) = current.as_mut() {
                    if ends_with(&path, &["trkpt", "ele"]) {
                        p.ele = Some(text.trim().to_string());
                    } else if ends_with(&path, &["trkpt", "time"]) {
                        p.time = Some(text.trim().to_string());
                    }
                }
                if ends_with(&path, &["trkseg", "trkpt"])
                    && let Some(p) = current.take()
                {
                    points.push(finish_point(p, points.len() + 1)?);
                }
                path.pop();
                text.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(Track {
        name: name
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| default_name.to_string()),
        points,
    })
}

fn local_name(e: &BytesStart) -> String {
    e.local_name().as_ref().to_string()
}

fn start_point(e: &BytesStart) -> Result<PartialPoint, String> {
    let attr = |key: &str| -> Result<Option<String>, String> {
        let value = e
            .try_get_attribute(key)
            .map_err(|err| err.to_string())?
            .map(|a| {
                a.normalized_value(XmlVersion::Implicit1_0)
                    .map(|v| v.into_owned())
            })
            .transpose()
            .map_err(|err| err.to_string())?;
        Ok(value)
    };
    Ok(PartialPoint {
        lat: attr("lat")?,
        lon: attr("lon")?,
        ..Default::default()
    })
}

/// `index` は 1 始まりの点の番号。エラーメッセージで何番目の点かを示すのに使う
fn finish_point(p: PartialPoint, index: usize) -> Result<Point, String> {
    let required = |value: Option<String>, label: &str| {
        value.ok_or_else(|| format!("{index} 番目の点に {label} がありません"))
    };
    let number = |value: String, label: &str| {
        value
            .parse::<f64>()
            .map_err(|_| format!("{index} 番目の点の {label} を数値として読めません: {value:?}"))
    };
    Ok(Point {
        lat: number(required(p.lat, "lat")?, "lat")?,
        lon: number(required(p.lon, "lon")?, "lon")?,
        ele: number(required(p.ele, "ele")?, "ele")?,
        time: parse_time(&required(p.time, "time")?)
            .ok_or_else(|| format!("{index} 番目の点の time を時刻として読めません"))?,
    })
}

/// オフセットの無い時刻は UTC として解釈する
fn parse_time(s: &str) -> Option<Timestamp> {
    if let Ok(ts) = s.parse::<Timestamp>() {
        return Some(ts);
    }
    let dt = s.parse::<DateTime>().ok()?;
    dt.to_zoned(TimeZone::UTC).ok().map(|z| z.timestamp())
}

fn ends_with(path: &[String], names: &[&str]) -> bool {
    path.len() >= names.len()
        && path[path.len() - names.len()..]
            .iter()
            .zip(names)
            .all(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpx(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?><gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">{body}</gpx>"#
        )
    }

    #[test]
    fn reads_points_across_tracks_and_segments_in_order() {
        let xml = gpx(r#"
            <metadata><name>metadata name</name></metadata>
            <wpt lat="1" lon="1"><ele>1</ele><time>2026-01-01T00:00:00Z</time></wpt>
            <trk><name>A &amp; B</name>
              <trkseg>
                <trkpt lat="35.0" lon="139.0"><ele>100</ele><time>2026-01-01T00:00:00Z</time></trkpt>
              </trkseg>
              <trkseg>
                <trkpt lat="35.1" lon="139.1"><ele>110.5</ele><time>2026-01-01T00:01:00Z</time></trkpt>
              </trkseg>
            </trk>
            <trk><name>second</name>
              <trkseg>
                <trkpt lat="35.2" lon="139.2"><ele>120</ele><time>2026-01-01T00:02:00</time></trkpt>
              </trkseg>
            </trk>"#);
        let track = parse(&xml, "file.gpx").unwrap();
        assert_eq!(track.name, "A & B");
        let lats: Vec<f64> = track.points.iter().map(|p| p.lat).collect();
        assert_eq!(lats, vec![35.0, 35.1, 35.2]);
        assert_eq!(track.points[1].ele, 110.5);
        // オフセットの無い時刻は UTC
        assert_eq!(
            track.points[2].time,
            "2026-01-01T00:02:00Z".parse::<Timestamp>().unwrap()
        );
    }

    #[test]
    fn falls_back_to_default_name() {
        let xml = gpx(r#"<trk><trkseg>
            <trkpt lat="35" lon="139"><ele>1</ele><time>2026-01-01T00:00:00Z</time></trkpt>
            </trkseg></trk>"#);
        assert_eq!(parse(&xml, "file.gpx").unwrap().name, "file.gpx");
    }

    #[test]
    fn reports_index_of_point_missing_required_value() {
        let xml = gpx(r#"<trk><trkseg>
            <trkpt lat="35" lon="139"><ele>1</ele><time>2026-01-01T00:00:00Z</time></trkpt>
            <trkpt lat="35" lon="139"><time>2026-01-01T00:00:00Z</time></trkpt>
            </trkseg></trk>"#);
        assert_eq!(
            parse(&xml, "f").unwrap_err(),
            "2 番目の点に ele がありません"
        );

        let xml = gpx(
            r#"<trk><trkseg><trkpt lat="35"><ele>1</ele><time>2026-01-01T00:00:00Z</time></trkpt></trkseg></trk>"#,
        );
        assert_eq!(
            parse(&xml, "f").unwrap_err(),
            "1 番目の点に lon がありません"
        );

        let xml = gpx(r#"<trk><trkseg><trkpt lat="35" lon="139"/></trkseg></trk>"#);
        assert_eq!(
            parse(&xml, "f").unwrap_err(),
            "1 番目の点に ele がありません"
        );
    }

    #[test]
    fn rejects_unreadable_values() {
        let xml = gpx(
            r#"<trk><trkseg><trkpt lat="35" lon="139"><ele>high</ele><time>2026-01-01T00:00:00Z</time></trkpt></trkseg></trk>"#,
        );
        assert_eq!(
            parse(&xml, "f").unwrap_err(),
            r#"1 番目の点の ele を数値として読めません: "high""#
        );

        let xml = gpx(
            r#"<trk><trkseg><trkpt lat="35" lon="139"><ele>1</ele><time>yesterday</time></trkpt></trkseg></trk>"#,
        );
        assert_eq!(
            parse(&xml, "f").unwrap_err(),
            "1 番目の点の time を時刻として読めません"
        );
    }
}
