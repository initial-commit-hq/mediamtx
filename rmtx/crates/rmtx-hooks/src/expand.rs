//! Expand `$VAR` / `${VAR}` in hook command strings (Go `os.Expand` subset).

use std::collections::HashMap;
use std::env;

/// Expands `$VAR` and `${VAR}` using `env` first, then the process environment.
///
/// Matches MediaMTX `externalcmd.NewCmd`, which calls `os.Expand` before exec.
pub fn expand_command(cmd: &str, env: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(cmd.len());
    let mut chars = cmd.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '$' {
            out.push(ch);
            continue;
        }

        if chars.peek() == Some(&'{') {
            chars.next();
            let var = take_until(&mut chars, '}');
            out.push_str(&lookup(&var, env));
            continue;
        }

        let var = take_ident(&mut chars);
        out.push_str(&lookup(&var, env));
    }

    out
}

fn lookup(name: &str, env: &HashMap<String, String>) -> String {
    if name.is_empty() {
        return String::new();
    }
    env.get(name)
        .cloned()
        .or_else(|| env::var(name).ok())
        .unwrap_or_default()
}

fn take_ident(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut var = String::new();
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            var.push(ch);
            chars.next();
        } else {
            break;
        }
    }
    var
}

fn take_until(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, end: char) -> String {
    let mut var = String::new();
    for ch in chars.by_ref() {
        if ch == end {
            break;
        }
        var.push(ch);
    }
    var
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_dollar_and_braced() {
        let mut env = HashMap::new();
        env.insert("MTX_PATH".into(), "mystream".into());
        env.insert("RTSP_PORT".into(), "8554".into());

        assert_eq!(
            expand_command("echo $MTX_PATH ${RTSP_PORT}", &env),
            "echo mystream 8554"
        );
    }

    #[test]
    fn expand_falls_back_to_process_env() {
        let key = format!("RMTX_HOOKS_TEST_{}", std::process::id());
        env::set_var(&key, "from_process");

        let env = HashMap::new();
        let cmd = format!("echo ${key}");
        assert_eq!(expand_command(&cmd, &env), "echo from_process");

        env::remove_var(&key);
    }

    #[test]
    fn expand_unknown_is_empty() {
        let env = HashMap::new();
        assert_eq!(expand_command("echo $UNKNOWN", &env), "echo ");
    }
}
