//! Async hook process runner (`tokio::process::Command`).

use std::collections::HashMap;
use std::process::Stdio;

use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::time::{sleep, Duration};

use crate::expand::expand_command;

/// Pause between restarts when `restart=true` (Go `externalcmd.restartPause`).
pub const RESTART_PAUSE: Duration = Duration::from_secs(5);

/// Handle to a background hook process (or restart loop).
#[derive(Debug)]
pub struct HookHandle {
    abort_tx: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl HookHandle {
    /// Signals the hook to stop. Sends `kill` to the child process when possible.
    ///
    /// Go sends `SIGINT` to the process group; Phase 1 uses `Child::start_kill` (Unix `SIGKILL`).
    pub fn abort(&self) {
        let _ = self.abort_tx.send(true);
    }

    /// Waits until the hook task finishes (mostly for tests).
    pub async fn wait(self) {
        let _ = self.task.await;
    }
}

/// Spawns a hook command in the background.
///
/// Variable expansion uses [`expand_command`] (Go `os.Expand`). The expanded command is executed via:
/// - **Unix:** `sh -c <cmd>`
/// - **Windows:** `cmd /C <cmd>`
///
/// The process inherits the full environment; `env` entries are appended (overriding duplicates).
/// When `restart` is true, the command is re-run after [`RESTART_PAUSE`] until [`HookHandle::abort`].
pub fn spawn_hook(cmd: &str, restart: bool, env: HashMap<String, String>) -> HookHandle {
    let expanded = expand_command(cmd, &env);
    let (abort_tx, abort_rx) = watch::channel(false);
    let task = tokio::spawn(run_hook(expanded, restart, env, abort_rx));

    HookHandle { abort_tx, task }
}

async fn run_hook(
    cmd: String,
    restart: bool,
    env: HashMap<String, String>,
    mut abort_rx: watch::Receiver<bool>,
) {
    loop {
        if *abort_rx.borrow() {
            return;
        }

        let mut child = match spawn_shell(&cmd, &env) {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(%err, "hook command failed to start");
                if !restart {
                    return;
                }
                if wait_or_abort(RESTART_PAUSE, &mut abort_rx).await {
                    return;
                }
                continue;
            }
        };

        let pid = child.id();

        tokio::select! {
            status = child.wait() => {
                match status {
                    Ok(exit) if !exit.success() => {
                        tracing::debug!(?exit, "hook command exited with error");
                    }
                    Err(err) => tracing::warn!(%err, "hook wait failed"),
                    _ => {}
                }
            }
            _ = abort_rx.changed() => {
                if *abort_rx.borrow() {
                    kill_child(&mut child, pid).await;
                    return;
                }
            }
        }

        if !restart {
            return;
        }

        if *abort_rx.borrow() {
            return;
        }

        if wait_or_abort(RESTART_PAUSE, &mut abort_rx).await {
            return;
        }
    }
}

fn spawn_shell(cmd: &str, env: &HashMap<String, String>) -> std::io::Result<Child> {
    let mut command = shell_command(cmd);
    command.stdin(Stdio::null());
    command.stdout(Stdio::inherit());
    command.stderr(Stdio::inherit());
    for (key, val) in env {
        command.env(key, val);
    }
    command.spawn()
}

fn shell_command(cmd: &str) -> Command {
    #[cfg(unix)]
    {
        let mut command = Command::new("sh");
        command.arg("-c").arg(cmd);
        command
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(cmd);
        command
    }
}

async fn kill_child(child: &mut Child, pid: Option<u32>) {
    if child.start_kill().is_ok() {
        let _ = child.wait().await;
        return;
    }
    if let Some(pid) = pid {
        tracing::debug!(pid, "hook child already exited");
    }
}

async fn wait_or_abort(duration: Duration, abort_rx: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = sleep(duration) => false,
        changed = abort_rx.changed() => {
            changed.is_ok() && *abort_rx.borrow()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::{env_on_connect, env_path_base, PathBaseEnv};
    use std::fs;
    use std::path::PathBuf;

    fn temp_outfile(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rmtx-hooks-{prefix}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir.join("out.txt")
    }

    #[tokio::test]
    async fn spawn_hook_injects_env() {
        let outfile = temp_outfile("env");
        let _ = fs::remove_file(&outfile);

        let base = PathBaseEnv::new("mypath", "8554");
        let env = env_path_base(&base);
        let out = outfile.display();
        let cmd = format!(
            "printenv MTX_PATH > {out} && printenv RTSP_PATH >> {out} && printenv RTSP_PORT >> {out}"
        );

        let handle = spawn_hook(&cmd, false, env);
        handle.wait().await;

        let content = fs::read_to_string(&outfile).expect("hook output file");
        assert!(content.contains("mypath"));
        assert!(content.contains("8554"));
    }

    #[tokio::test]
    async fn spawn_hook_expands_variables() {
        let outfile = temp_outfile("expand");
        let _ = fs::remove_file(&outfile);

        let mut env = HashMap::new();
        env.insert("MTX_PATH".into(), "expanded-name".into());
        let cmd = format!("echo $MTX_PATH > {}", outfile.display());

        let handle = spawn_hook(&cmd, false, env);
        handle.wait().await;

        let content = fs::read_to_string(&outfile).unwrap();
        assert!(content.contains("expanded-name"));
    }

    #[tokio::test]
    async fn spawn_hook_on_connect_env() {
        let outfile = temp_outfile("connect");
        let _ = fs::remove_file(&outfile);

        let env = env_on_connect("rtspConn", "conn-99", "8554");
        let out = outfile.display();
        let cmd = format!("printenv MTX_CONN_TYPE > {out} && printenv MTX_CONN_ID >> {out}");

        let handle = spawn_hook(&cmd, false, env);
        handle.wait().await;

        let content = fs::read_to_string(&outfile).unwrap();
        assert!(content.contains("rtspConn"));
        assert!(content.contains("conn-99"));
    }

    #[tokio::test]
    async fn abort_stops_hook() {
        let env = HashMap::new();
        let handle = spawn_hook("sleep 60", false, env);
        sleep(Duration::from_millis(100)).await;
        handle.abort();
        handle.wait().await;
    }
}
