//! JSON views and patch helpers for [`rmtx_conf::Conf`] (Phase 1 API glue).

use rmtx_conf::{Conf, Path};
use serde_json::{json, Value};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigViewError {
    #[error("path already exists")]
    PathAlreadyExists,
    #[error("path configuration not found")]
    PathNotFound,
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

/// Returns the global configuration object (everything except paths and pathDefaults).
pub fn global_json(conf: &Conf) -> Value {
    let mut value = serde_json::to_value(conf).expect("Conf serializes");
    if let Some(obj) = value.as_object_mut() {
        obj.remove("paths");
        obj.remove("pathDefaults");
    }
    value
}

/// Returns path defaults as JSON.
pub fn path_defaults_json(conf: &Conf) -> Value {
    serde_json::to_value(&conf.path_defaults).expect("Path serializes")
}

/// Returns a path configuration with the `name` field set.
pub fn path_json(conf: &Conf, name: &str) -> Option<Value> {
    conf.paths.get(name).map(|p| {
        let mut value = serde_json::to_value(p).expect("Path serializes");
        if let Some(obj) = value.as_object_mut() {
            obj.insert("name".to_owned(), json!(name));
        }
        value
    })
}

/// Sorted path configuration list items (each includes `name`).
pub fn paths_list_json(conf: &Conf) -> Vec<Value> {
    let mut names: Vec<_> = conf.paths.keys().cloned().collect();
    names.sort();
    names
        .into_iter()
        .filter_map(|name| path_json(conf, &name))
        .collect()
}

pub fn patch_global(conf: &mut Conf, patch: Value) -> Result<(), ConfigViewError> {
    let paths = conf.paths.clone();
    let path_defaults = conf.path_defaults.clone();
    let mut merged = global_json(conf);
    merge_value(&mut merged, patch);

    let mut full = merged;
    if let Some(obj) = full.as_object_mut() {
        obj.insert(
            "pathDefaults".to_owned(),
            serde_json::to_value(&path_defaults)
                .map_err(|e| ConfigViewError::Invalid(e.to_string()))?,
        );
        obj.insert(
            "paths".to_owned(),
            serde_json::to_value(&paths).map_err(|e| ConfigViewError::Invalid(e.to_string()))?,
        );
    }

    let updated: Conf =
        serde_json::from_value(full).map_err(|e| ConfigViewError::Invalid(e.to_string()))?;
    *conf = updated;
    Ok(())
}

pub fn patch_path_defaults(conf: &mut Conf, patch: Value) -> Result<(), ConfigViewError> {
    let mut value = path_defaults_json(conf);
    merge_value(&mut value, patch);
    conf.path_defaults =
        serde_json::from_value(value).map_err(|e| ConfigViewError::Invalid(e.to_string()))?;
    Ok(())
}

pub fn add_path(conf: &mut Conf, name: &str, body: Value) -> Result<(), ConfigViewError> {
    if conf.paths.contains_key(name) {
        return Err(ConfigViewError::PathAlreadyExists);
    }
    let mut path: Path =
        serde_json::from_value(body).map_err(|e| ConfigViewError::Invalid(e.to_string()))?;
    path.name = name.to_owned();
    conf.paths.insert(name.to_owned(), path);
    Ok(())
}

pub fn patch_path(conf: &mut Conf, name: &str, patch: Value) -> Result<(), ConfigViewError> {
    let entry = conf
        .paths
        .get_mut(name)
        .ok_or(ConfigViewError::PathNotFound)?;
    let mut value =
        serde_json::to_value(&*entry).map_err(|e| ConfigViewError::Invalid(e.to_string()))?;
    merge_value(&mut value, patch);
    let updated: Path =
        serde_json::from_value(value).map_err(|e| ConfigViewError::Invalid(e.to_string()))?;
    *entry = updated;
    entry.name = name.to_owned();
    Ok(())
}

pub fn replace_path(conf: &mut Conf, name: &str, body: Value) -> Result<(), ConfigViewError> {
    if !conf.paths.contains_key(name) {
        return Err(ConfigViewError::PathNotFound);
    }
    let mut path: Path =
        serde_json::from_value(body).map_err(|e| ConfigViewError::Invalid(e.to_string()))?;
    path.name = name.to_owned();
    conf.paths.insert(name.to_owned(), path);
    Ok(())
}

pub fn remove_path(conf: &mut Conf, name: &str) -> Result<(), ConfigViewError> {
    conf.paths
        .remove(name)
        .ok_or(ConfigViewError::PathNotFound)?;
    Ok(())
}

pub fn api_allow_origins(_conf: &Conf) -> Option<Vec<String>> {
    // Phase 1: field not modeled in rmtx-conf yet; use permissive CORS.
    None
}

fn merge_value(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            for (key, patch_val) in patch_map {
                match target_map.get_mut(&key) {
                    Some(existing) if existing.is_object() && patch_val.is_object() => {
                        merge_value(existing, patch_val);
                    }
                    Some(slot) => *slot = patch_val,
                    None => {
                        target_map.insert(key, patch_val);
                    }
                }
            }
        }
        (target_slot, patch) => *target_slot = patch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_patch_log_level() {
        let mut conf = Conf::default();
        patch_global(&mut conf, json!({"logLevel": "debug"})).unwrap();
        assert_eq!(conf.log_level, "debug");
    }

    #[test]
    fn path_add_patch_delete() {
        let mut conf = Conf::default();
        add_path(&mut conf, "cam", json!({"source": "rtsp://x"})).unwrap();
        patch_path(&mut conf, "cam", json!({"record": true})).unwrap();
        assert!(conf.paths["cam"].record);
        remove_path(&mut conf, "cam").unwrap();
        assert!(!conf.paths.contains_key("cam"));
    }
}
