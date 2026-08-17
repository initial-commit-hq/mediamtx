use chrono::{DateTime, Datelike, Timelike, Utc};

use crate::RecordFormat;

fn leading_zeros(value: u32, width: usize) -> String {
    format!("{value:0width$}")
}

fn timezone_offset_token(time: DateTime<Utc>) -> String {
    let offset = time.format("%z").to_string();
    if offset == "+0000" {
        "Z".to_owned()
    } else {
        format!("{}{}:{}", &offset[..3], &offset[3..5], &offset[5..])
    }
}

/// Substitutes `%path` and strftime-like tokens in a recording path template.
///
/// Supported tokens: `%path`, `%Y`, `%m`, `%d`, `%H`, `%M`, `%S`, `%f`, `%z`, `%s`.
pub fn encode_segment_path(template: &str, path_name: &str, start: DateTime<Utc>) -> String {
    template
        .replace("%path", path_name)
        .replace("%Y", &start.year().to_string())
        .replace("%m", &leading_zeros(start.month(), 2))
        .replace("%d", &leading_zeros(start.day(), 2))
        .replace("%H", &leading_zeros(start.hour(), 2))
        .replace("%M", &leading_zeros(start.minute(), 2))
        .replace("%S", &leading_zeros(start.second(), 2))
        .replace("%f", &leading_zeros(start.timestamp_subsec_micros(), 6))
        .replace("%z", &timezone_offset_token(start))
        .replace("%s", &start.timestamp().to_string())
}

/// Appends the container-specific file extension (`.mp4` or `.ts`).
pub fn add_format_extension(path: &str, format: RecordFormat) -> String {
    match format {
        RecordFormat::Fmp4 => format!("{path}.mp4"),
        RecordFormat::Mpegts => format!("{path}.ts"),
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn path_template_substitutes_path_and_timestamp() {
        let start = Utc.with_ymd_and_hms(2024, 3, 15, 10, 30, 45).unwrap();
        let encoded = encode_segment_path("./recordings/%path/%Y-%m-%d_%H-%M-%S-%f", "cam1", start);
        assert_eq!(encoded, "./recordings/cam1/2024-03-15_10-30-45-000000");
    }

    #[test]
    fn add_format_extension_appends_container_suffix() {
        let base = "./recordings/cam1/2024-03-15_10-30-45-000000";
        assert_eq!(
            add_format_extension(base, RecordFormat::Fmp4),
            "./recordings/cam1/2024-03-15_10-30-45-000000.mp4"
        );
        assert_eq!(
            add_format_extension(base, RecordFormat::Mpegts),
            "./recordings/cam1/2024-03-15_10-30-45-000000.ts"
        );
    }
}
