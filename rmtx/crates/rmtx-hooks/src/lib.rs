//! Lifecycle hooks with MediaMTX env contract.
//!
//! Go counterpart: `internal/hooks` + `internal/externalcmd`.
//!
//! Phase 1: env builders + async hook executor with variable expansion.

#![forbid(unsafe_code)]

mod env;
mod executor;
mod expand;
mod runner;

pub use env::{
    env_on_connect, env_on_demand, env_on_read, env_on_ready, env_on_record_segment,
    env_on_source_connect, env_path_base, PathBaseEnv,
};
pub use executor::{spawn_hook, HookHandle, RESTART_PAUSE};
pub use expand::expand_command;
pub use runner::{rtsp_port_from_address, HookRunner, PathReadyHooks};
