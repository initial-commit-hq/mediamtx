//! Serde helpers for MediaMTX YAML quirks (null command fields, etc.).

use serde::{Deserialize, Deserializer};

/// Deserialize a YAML null or absent value as an empty string.
pub fn null_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(|v| v.unwrap_or_default())
}
