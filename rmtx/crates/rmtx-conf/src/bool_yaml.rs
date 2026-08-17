//! MediaMTX-style booleans: `yes`/`no`/`true`/`false`.

use serde::de::{self, Deserializer, Unexpected};
use serde::Deserialize;

pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_yaml::Value::deserialize(deserializer)?;
    match value {
        serde_yaml::Value::Bool(b) => Ok(b),
        serde_yaml::Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "yes" | "true" | "1" | "on" => Ok(true),
            "no" | "false" | "0" | "off" | "" => Ok(false),
            other => Err(de::Error::invalid_value(
                Unexpected::Str(other),
                &"yes, no, true, or false",
            )),
        },
        serde_yaml::Value::Number(n) => n
            .as_u64()
            .map(|v| v != 0)
            .ok_or_else(|| de::Error::custom("invalid numeric boolean")),
        serde_yaml::Value::Null => Ok(false),
        _ => Err(de::Error::custom("expected yes, no, true, or false")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Wrap {
        #[serde(deserialize_with = "deserialize")]
        flag: bool,
    }

    #[test]
    fn yes_no_strings() {
        let w: Wrap = serde_yaml::from_str("flag: yes").unwrap();
        assert!(w.flag);
        let w: Wrap = serde_yaml::from_str("flag: no").unwrap();
        assert!(!w.flag);
    }
}
