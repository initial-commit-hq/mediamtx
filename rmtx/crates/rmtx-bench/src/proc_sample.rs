//! Process RSS / CPU sampling (macOS and Linux).

use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Serialize)]
pub struct ProcSample {
    pub rss_kb_max: u64,
    pub cpu_pct_max: f64,
    pub samples: u64,
}

#[derive(Debug, Error)]
pub enum ProcError {
    #[error("unsupported platform for proc sampling")]
    Unsupported,

    #[error("process {0} not found")]
    NotFound(u32),

    #[error("failed to read process stats: {0}")]
    Io(String),
}

/// Polls `pid` until `duration` elapses, tracking peak RSS and CPU%.
pub async fn sample_process(pid: u32, duration: Duration, interval: Duration) -> Result<ProcSample, ProcError> {
    if duration.is_zero() {
        return Ok(ProcSample {
            rss_kb_max: 0,
            cpu_pct_max: 0.0,
            samples: 0,
        });
    }

    let started = Instant::now();
    let mut rss_kb_max = 0u64;
    let mut cpu_pct_max = 0.0f64;
    let mut samples = 0u64;
    let mut prev = read_proc_stat(pid)?;

    while started.elapsed() < duration {
        tokio::time::sleep(interval).await;
        let cur = read_proc_stat(pid)?;
        samples += 1;
        rss_kb_max = rss_kb_max.max(cur.rss_kb);
        let cpu_pct = cpu_delta_pct(&prev, &cur, interval);
        cpu_pct_max = cpu_pct_max.max(cpu_pct);
        prev = cur;
    }

    Ok(ProcSample {
        rss_kb_max,
        cpu_pct_max,
        samples,
    })
}

struct ProcStat {
    rss_kb: u64,
    utime: u64,
    stime: u64,
}

fn cpu_delta_pct(prev: &ProcStat, cur: &ProcStat, interval: Duration) -> f64 {
    #[cfg(target_os = "macos")]
    {
        let _ = (prev, interval);
        return cur.utime as f64;
    }
    #[cfg(not(target_os = "macos"))]
    {
        let dt_us = interval.as_micros().max(1) as f64;
        let d_ticks = cur.utime.saturating_sub(prev.utime) + cur.stime.saturating_sub(prev.stime);
        (d_ticks as f64 / dt_us) * 100.0 * 10_000.0
    }
}

#[cfg(target_os = "linux")]
fn read_proc_stat(pid: u32) -> Result<ProcStat, ProcError> {
    use std::fs;

    let status = fs::read_to_string(format!("/proc/{pid}/status"))
        .map_err(|_| ProcError::NotFound(pid))?;
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))
        .map_err(|e| ProcError::Io(e.to_string()))?;

    let rss_kb = status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let fields: Vec<&str> = stat.split_whitespace().collect();
    if fields.len() < 17 {
        return Err(ProcError::Io("short /proc stat".into()));
    }
    let utime = fields[13].parse().unwrap_or(0);
    let stime = fields[14].parse().unwrap_or(0);
    Ok(ProcStat {
        rss_kb,
        utime,
        stime,
    })
}

#[cfg(target_os = "macos")]
fn read_proc_stat(pid: u32) -> Result<ProcStat, ProcError> {
    use std::process::Command;

    let out = Command::new("ps")
        .args(["-o", "rss=,pcpu=", "-p", &pid.to_string()])
        .output()
        .map_err(|e| ProcError::Io(e.to_string()))?;
    if !out.status.success() {
        return Err(ProcError::NotFound(pid));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = text.split_whitespace().collect();
    let rss_kb = parts.first().and_then(|v| v.parse().ok()).unwrap_or(0);
    let cpu_pct = parts.get(1).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);

    Ok(ProcStat {
        rss_kb,
        utime: cpu_pct as u64,
        stime: 0,
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn read_proc_stat(_pid: u32) -> Result<ProcStat, ProcError> {
    Err(ProcError::Unsupported)
}
