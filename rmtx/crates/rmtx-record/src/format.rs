use std::str::FromStr;

use crate::RecordError;

/// Output container format for recordings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordFormat {
    Fmp4,
    Mpegts,
}

impl FromStr for RecordFormat {
    type Err = RecordError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "fmp4" => Ok(Self::Fmp4),
            "mpegts" | "mpeg-ts" | "ts" => Ok(Self::Mpegts),
            other => Err(RecordError::UnknownFormat(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fmp4_and_mpegts() {
        assert_eq!("fmp4".parse::<RecordFormat>().unwrap(), RecordFormat::Fmp4);
        assert_eq!("FMP4".parse::<RecordFormat>().unwrap(), RecordFormat::Fmp4);
        assert_eq!(
            "mpegts".parse::<RecordFormat>().unwrap(),
            RecordFormat::Mpegts
        );
        assert_eq!(
            "mpeg-ts".parse::<RecordFormat>().unwrap(),
            RecordFormat::Mpegts
        );
        assert_eq!("ts".parse::<RecordFormat>().unwrap(), RecordFormat::Mpegts);
    }

    #[test]
    fn parse_unknown_format_errors() {
        let err = "webm".parse::<RecordFormat>().unwrap_err();
        assert!(matches!(err, RecordError::UnknownFormat(ref s) if s == "webm"));
    }
}
