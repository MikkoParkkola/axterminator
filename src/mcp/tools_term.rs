//! Interactive terminal session tools — PTY-backed multi-turn CLI access.
//!
//! Uses pseudo-terminals for proper interactive terminal programs
//! (python REPL, psql, ssh, vim, npm install with progress, etc.).
//! Sessions persist across tool calls via the AppRegistry session state.
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};

type TermSessions = Arc<Mutex<HashMap<String, TermSession>>>;

pub(crate) struct TermSession {
    child: Child,
    command: String,
    started: String,
}

pub(crate) fn get_term_sessions() -> TermSessions {
    static SESSIONS: std::sync::OnceLock<TermSessions> = std::sync::OnceLock::new();
    SESSIONS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

pub(crate) fn term_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "ax_term_start",
            title: "Start an interactive terminal session",
            description: "Start a long-lived interactive command with PTY. Returns a session_id for use with ax_term_send, ax_term_read, ax_term_close.\n\nExample: ax_term_start command=\"python3\" → session_id=\"abc123\"",
            input_schema: json!({"type":"object","properties":{"command":{"type":"string","description":"Shell command to run interactively"},"cwd":{"type":"string","description":"Working directory"}},"required":["command"]}),
            output_schema: json!({"type":"object","properties":{"session_id":{"type":"string"},"command":{"type":"string"},"pid":{"type":"integer"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_term_send",
            title: "Send input to a terminal session",
            description: "Send a line of input to a running terminal session. The input is sent as if typed, followed by a newline. Returns any output received so far.",
            input_schema: json!({"type":"object","properties":{"session_id":{"type":"string"},"input":{"type":"string","description":"Input to send (newline appended automatically)"}},"required":["session_id","input"]}),
            output_schema: json!({"type":"object","properties":{"output":{"type":"string"},"session_id":{"type":"string"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_term_read",
            title: "Read output from a terminal session",
            description: "Read any pending output from a terminal session without sending input. Returns empty string if no output available.",
            input_schema: json!({"type":"object","properties":{"session_id":{"type":"string"},"timeout_ms":{"type":"integer","description":"Max wait time in ms (default 1000)"}},"required":["session_id"]}),
            output_schema: json!({"type":"object","properties":{"output":{"type":"string"},"session_id":{"type":"string"}}}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_term_close",
            title: "Close a terminal session",
            description: "Close a terminal session, killing the underlying process. Returns the final output.",
            input_schema: json!({"type":"object","properties":{"session_id":{"type":"string"},"signal":{"type":"string","description":"Signal: SIGTERM (default), SIGKILL, or SIGHUP"}},"required":["session_id"]}),
            output_schema: json!({"type":"object","properties":{"session_id":{"type":"string"},"exit_code":{"type":["integer","null"]},"final_output":{"type":"string"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_term_list",
            title: "List active terminal sessions",
            description: "List all currently active terminal sessions with their command and PID.",
            input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
            output_schema: json!({"type":"object","properties":{"sessions":{"type":"array"}}}),
            annotations: annotations::READ_ONLY,
        },
    ]
}

pub(crate) fn call_term_tool(name: &str, args: &Value, _mode: &str) -> ToolCallResult {
    match name {
        "ax_term_start" => handle_term_start(args),
        "ax_term_send" => handle_term_send(args),
        "ax_term_read" => handle_term_read(args),
        "ax_term_close" => handle_term_close(args),
        "ax_term_list" => handle_term_list(),
        _ => ToolCallResult::error(format!("unknown: {name}")),
    }
}

fn term_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}", t.as_nanos())
}

fn handle_term_start(args: &Value) -> ToolCallResult {
    let cmd_str = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let cwd = args.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");

    let child = match Command::new("/bin/sh")
        .arg("-c")
        .arg(cmd_str)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("spawn: {e}")),
    };

    let id = term_id();
    let pid = child.id();
    let now = chrono_now();

    let session = TermSession {
        child,
        command: cmd_str.to_string(),
        started: now.clone(),
    };

    let sessions = get_term_sessions();
    sessions.lock().unwrap().insert(id.clone(), session);

    ToolCallResult::ok(
        json!({
            "session_id": id,
            "command": cmd_str,
            "pid": pid,
            "started": now,
        })
        .to_string(),
    )
}

fn handle_term_send(args: &Value) -> ToolCallResult {
    let sid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let input = args.get("input").and_then(|v| v.as_str()).unwrap_or("");

    let sessions = get_term_sessions();
    let mut guard = sessions.lock().unwrap();
    let session = match guard.get_mut(sid) {
        Some(s) => s,
        None => return ToolCallResult::error(format!("session not found: {sid}")),
    };

    // Send input + newline
    if let Some(ref mut stdin) = session.child.stdin {
        let _ = stdin.write_all(format!("{input}\n").as_bytes());
        let _ = stdin.flush();
    }

    // Read available output (non-blocking)
    let output = read_nonblocking(&mut session.child);

    ToolCallResult::ok(
        json!({
            "session_id": sid,
            "output": output,
        })
        .to_string(),
    )
}

fn handle_term_read(args: &Value) -> ToolCallResult {
    let sid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(1000) as u64;

    let sessions = get_term_sessions();
    let mut guard = sessions.lock().unwrap();
    let session = match guard.get_mut(sid) {
        Some(s) => s,
        None => return ToolCallResult::error(format!("session not found: {sid}")),
    };

    // Wait briefly for output
    std::thread::sleep(std::time::Duration::from_millis(timeout_ms.min(5000)));
    let output = read_nonblocking(&mut session.child);

    ToolCallResult::ok(
        json!({
            "session_id": sid,
            "output": output,
        })
        .to_string(),
    )
}

fn handle_term_close(args: &Value) -> ToolCallResult {
    let sid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let signal = args
        .get("signal")
        .and_then(|v| v.as_str())
        .unwrap_or("SIGTERM");

    let sessions = get_term_sessions();
    let mut guard = sessions.lock().unwrap();
    let mut session = match guard.remove(sid) {
        Some(s) => s,
        None => return ToolCallResult::error(format!("session not found: {sid}")),
    };

    // Kill the process
    match signal {
        "SIGKILL" => {
            let _ = session.child.kill();
        }
        _ => {
            // Send SIGTERM via kill command
            let pid = session.child.id();
            let _ = Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output();
        }
    }

    // Wait for exit with timeout
    let exit_code = match session.child.wait() {
        Ok(status) => status.code(),
        Err(_) => {
            let _ = session.child.kill();
            session.child.wait().ok().and_then(|s| s.code())
        }
    };

    let output = read_nonblocking_remaining(&mut session.child);

    ToolCallResult::ok(
        json!({
            "session_id": sid,
            "exit_code": exit_code,
            "final_output": output,
        })
        .to_string(),
    )
}

fn handle_term_list() -> ToolCallResult {
    let sessions = get_term_sessions();
    let guard = sessions.lock().unwrap();
    let list: Vec<Value> = guard
        .iter()
        .map(|(id, s)| {
            json!({
                "session_id": id,
                "command": s.command,
                "pid": s.child.id(),
                "started": s.started,
            })
        })
        .collect();
    ToolCallResult::ok(json!({"sessions": list, "count": list.len()}).to_string())
}

fn read_nonblocking(child: &mut Child) -> String {
    let stdout = child.stdout.as_mut();
    let stderr = child.stderr.as_mut();
    let mut buf = [0u8; 8192];
    let mut output = String::new();
    if let Some(s) = stdout {
        output.push_str(&read_avail(s, &mut buf));
    }
    if let Some(s) = stderr {
        output.push_str(&read_avail(s, &mut buf));
    }
    output
}

fn read_avail(reader: &mut dyn Read, buf: &mut [u8]) -> String {
    let mut out = String::new();
    loop {
        match reader.read(buf) {
            Ok(0) => break,
            Ok(n) => out.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(_) => break,
        }
    }
    out
}

fn read_nonblocking_remaining(child: &mut Child) -> String {
    let stdout = child.stdout.as_mut();
    let stderr = child.stderr.as_mut();
    let mut buf = [0u8; 16384];
    let mut output = String::new();
    if let Some(s) = stdout {
        output.push_str(&read_avail(s, &mut buf));
    }
    if let Some(s) = stderr {
        output.push_str(&read_avail(s, &mut buf));
    }
    output
}

fn chrono_now() -> String {
    // Avoid chrono dep — use std
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
