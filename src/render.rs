pub mod json;
pub mod text;

#[cfg(test)]
pub(crate) mod tests {
    use jiff::tz::TimeZone;

    use crate::gpx;
    use crate::summary::{Summary, summarize};

    fn summary_of(xml: &str, file_name: &str) -> Summary {
        let track = gpx::parse(xml, file_name).unwrap();
        summarize(&track, &TimeZone::get("Asia/Tokyo").unwrap()).unwrap()
    }

    /// 2 泊 3 日で、中日に記録が無い片道の記録
    pub fn sample_summary() -> Summary {
        summary_of(include_str!("../tests/fixtures/sample.gpx"), "sample.gpx")
    }

    /// trk/name が無い日帰りの周回
    pub fn day_trip_summary() -> Summary {
        summary_of(
            include_str!("../tests/fixtures/day_trip.gpx"),
            "day_trip.gpx",
        )
    }
}
