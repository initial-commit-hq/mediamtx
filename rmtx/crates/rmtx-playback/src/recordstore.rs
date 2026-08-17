//! Scan on-disk recording segments for a path (Phase 2 scaffold).

use std::fs;
use std::path::{Path, PathBuf};

use std::str::FromStr;

use rmtx_path::PathConf;
use rmtx_record::RecordFormat;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize)]
pub struct ListEntry {
    pub start: String,
    pub duration: f64,
    pub url: String,
}

/// Lists recording files under the configured `record_path` for `path_name`.
pub fn list_segments(path_conf: &PathConf, path_name: &str) -> Vec<ListEntry> {
    let Some(dir) = recordings_dir(path_conf, path_name) else {
        return Vec::new();
    };
    if !dir.is_dir() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let Ok(read) = fs::read_dir(&dir) else {
        return entries;
    };

    for item in read.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "mp4" && ext != "ts" {
            continue;
        }
        let Ok(meta) = item.metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        let start = OffsetDateTime::from(modified);
        let start_str = start.format(&Rfc3339).unwrap_or_default();
        entries.push(ListEntry {
            start: start_str.clone(),
            duration: 0.0,
            url: format!("/get?path={path_name}&start={start_str}"),
        });
    }

    entries.sort_by(|a, b| a.start.cmp(&b.start));
    entries
}

/// Opens a segment file for playback `GET /get?path=&start=`.
pub fn open_segment(path_conf: &PathConf, path_name: &str, start: &str) -> Option<PathBuf> {
    let dir = recordings_dir(path_conf, path_name)?;
    let target = parse_rfc3339(start)?;
    let format = RecordFormat::from_str(&path_conf.record_format).ok();

    for item in fs::read_dir(&dir).ok()?.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Ok(modified) = item.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let modified = OffsetDateTime::from(modified);
        if modified.unix_timestamp() == target.unix_timestamp()
            || modified
                .format(&Rfc3339)
                .ok()
                .as_deref()
                .is_some_and(|s| s == start)
        {
            return Some(path);
        }
    }

    let _ = format;
    None
}

fn recordings_dir(path_conf: &PathConf, path_name: &str) -> Option<PathBuf> {
    let template = path_conf.record_path.replace("%path", path_name);
    let without_ext = template
        .strip_suffix(".mp4")
        .or_else(|| template.strip_suffix(".ts"))
        .unwrap_or(&template);
    let base = Path::new(without_ext);
    if base.extension().is_some() {
        base.parent().map(|p| p.to_path_buf())
    } else {
        Some(base.to_path_buf())
    }
}

fn parse_rfc3339(s: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(s, &Rfc3339).ok()
}
