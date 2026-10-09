use std::path::PathBuf;
use std::process::{Command, Output};

fn gpxcat(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gpxcat"))
        .args(args)
        .output()
        .unwrap()
}

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

/// テスト中だけ使う GPX を一時ディレクトリに書き出す
fn temp_gpx(name: &str, body: &str) -> String {
    let path = std::env::temp_dir().join(format!("gpxcat-test-{}-{name}", std::process::id()));
    std::fs::write(&path, body).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn succeeds_with_text_and_json() {
    let out = gpxcat(&[&fixture("sample.gpx")]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("合成テスト山行")
    );

    let out = gpxcat(&["--format", "json", "--tz", "UTC", &fixture("sample.gpx")]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["timezone"], "UTC");
    assert_eq!(json["start"], "2026-08-14T21:00:00+00:00");
}

#[test]
fn exits_with_1_when_file_cannot_be_read() {
    let out = gpxcat(&["no-such-file.gpx"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("no-such-file.gpx")
    );
}

#[test]
fn exits_with_1_and_reports_point_missing_required_value() {
    let path = temp_gpx(
        "missing-time.gpx",
        r#"<gpx><trk><trkseg>
            <trkpt lat="35" lon="139"><ele>1</ele><time>2026-01-01T00:00:00Z</time></trkpt>
            <trkpt lat="35" lon="139"><ele>1</ele></trkpt>
        </trkseg></trk></gpx>"#,
    );
    let out = gpxcat(&[&path]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("2 番目の点に time がありません")
    );
}

#[test]
fn exits_with_1_when_fewer_than_two_points() {
    let path = temp_gpx(
        "one-point.gpx",
        r#"<gpx><trk><trkseg>
            <trkpt lat="35" lon="139"><ele>1</ele><time>2026-01-01T00:00:00Z</time></trkpt>
        </trkseg></trk></gpx>"#,
    );
    assert_eq!(gpxcat(&[&path]).status.code(), Some(1));
}

#[test]
fn exits_with_2_on_invalid_arguments() {
    assert_eq!(gpxcat(&[]).status.code(), Some(2));
    assert_eq!(
        gpxcat(&["--format", "csv", &fixture("sample.gpx")])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        gpxcat(&["--tz", "Mars/Olympus", &fixture("sample.gpx")])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn help_shows_a_cat() {
    let out = gpxcat(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8(out.stdout).unwrap().contains("( o.o )"));
}

#[test]
fn detail_adds_splits() {
    let json = |args: &[&str]| -> serde_json::Value {
        serde_json::from_slice(&gpxcat(args).stdout).unwrap()
    };
    let path = fixture("day_trip.gpx");
    assert!(json(&["--format", "json", &path]).get("splits").is_none());
    assert_eq!(
        json(&["--format", "json", "--detail", &path])["splits"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let text = String::from_utf8(gpxcat(&["--detail", &path]).stdout).unwrap();
    assert!(text.contains("区間（1 km ごと"));
}
