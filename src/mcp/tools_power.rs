//! Additional power tools: file ops, HTTP, app management, notifications.
use serde_json::{Value, json};
use std::process::Command;

use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};

pub(crate) fn power_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "ax_fs_edit",
            title: "Find and replace in a file",
            description: "Replace all occurrences of a string in a file. Returns number of replacements. Creates a backup with .bak extension.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"find":{"type":"string"},"replace":{"type":"string"},"backup":{"type":"boolean","description":"Create .bak backup (default true)"}},"required":["path","find","replace"]}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_fs_search",
            title: "Search file contents",
            description: "Search for a pattern in files using grep. Returns matching files with line numbers and context.",
            input_schema: json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string","description":"File or directory path"},"max_results":{"type":"integer","description":"Max matches (default 50)"}},"required":["pattern"]}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_fs_delete",
            title: "Delete a file or empty directory",
            description: "Delete a file or empty directory at the given path. Returns success/failure.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"recursive":{"type":"boolean","description":"Recursively delete directories (default false)"}},"required":["path"]}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_http_get",
            title: "HTTP GET request",
            description: "Make an HTTP GET request and return status, headers, and body.",
            input_schema: json!({"type":"object","properties":{"url":{"type":"string"},"headers":{"type":"object","description":"Additional headers"}},"required":["url"]}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_app_launch",
            title: "Launch or quit an application",
            description: "Launch an app by name or bundle ID, or quit by PID/name.",
            input_schema: json!({"type":"object","properties":{"app":{"type":"string","description":"App name or bundle ID"},"action":{"type":"string","description":"launch or quit"}},"required":["app","action"]}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_notify",
            title: "Send a macOS notification",
            description: "Post a notification to the macOS Notification Center via osascript.",
            input_schema: json!({"type":"object","properties":{"title":{"type":"string"},"message":{"type":"string"},"sound":{"type":"string","description":"Sound name or 'default'"}},"required":["title","message"]}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::DESTRUCTIVE,
        },
    ]
}

pub(crate) fn call_power_tool(name: &str, args: &Value, _mode: &str) -> ToolCallResult {
    match name {
        "ax_fs_edit" => handle_edit(args),
        "ax_fs_search" => handle_search(args),
        "ax_fs_delete" => handle_delete(args),
        "ax_http_get" => handle_http(args),
        "ax_app_launch" => handle_app(args),
        "ax_notify" => handle_notify(args),
        _ => ToolCallResult::error(format!("unknown: {name}")),
    }
}

fn resolve_path(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return format!("{home}/{rest}");
    }
    raw.to_string()
}

fn handle_edit(args: &Value) -> ToolCallResult {
    let path = resolve_path(args.get("path").and_then(|v| v.as_str()).unwrap_or(""));
    let find = args.get("find").and_then(|v| v.as_str()).unwrap_or("");
    let replace = args.get("replace").and_then(|v| v.as_str()).unwrap_or("");
    let backup = args.get("backup").and_then(|v| v.as_bool()).unwrap_or(true);

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return ToolCallResult::error(format!("read: {e}")),
    };
    if !content.contains(find) {
        return ToolCallResult::ok(
            json!({"path":path,"replacements":0,"message":"no matches"}).to_string(),
        );
    }
    if backup {
        let _ = std::fs::copy(&path, format!("{path}.bak"));
    }
    let count = content.matches(find).count();
    let new_content = content.replace(find, replace);
    match std::fs::write(&path, &new_content) {
        Ok(()) => ToolCallResult::ok(
            json!({"path":path,"replacements":count,"backup_created":backup}).to_string(),
        ),
        Err(e) => ToolCallResult::error(format!("write: {e}")),
    }
}

fn handle_search(args: &Value) -> ToolCallResult {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .map(resolve_path)
        .unwrap_or_else(|| ".".into());
    let max = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(50);

    match Command::new("grep")
        .args(["-rn", "--include=*", "-m", &max.to_string(), pattern, &path])
        .output()
    {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<&str> = text.lines().take(max as usize).collect();
            ToolCallResult::ok(
                json!({"pattern":pattern,"matches":lines.len(),"results":lines}).to_string(),
            )
        }
        Err(_) => {
            ToolCallResult::ok(json!({"pattern":pattern,"matches":0,"results":[]}).to_string())
        }
    }
}

fn handle_delete(args: &Value) -> ToolCallResult {
    let path = resolve_path(args.get("path").and_then(|v| v.as_str()).unwrap_or(""));
    let recursive = args
        .get("recursive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let result = if recursive {
        std::fs::remove_dir_all(&path)
    } else if std::fs::metadata(&path)
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        std::fs::remove_dir(&path)
    } else {
        std::fs::remove_file(&path)
    };
    match result {
        Ok(()) => ToolCallResult::ok(json!({"path":path,"deleted":true}).to_string()),
        Err(e) => ToolCallResult::error(format!("delete: {e}")),
    }
}

fn handle_http(args: &Value) -> ToolCallResult {
    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
    // Use curl for HTTP (no new deps)
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",
        "-o",
        "/dev/stdout",
        "-w",
        "\n__STATUS__%{http_code}__END__",
        "--max-time",
        "30",
        "-L",
        url,
    ]);
    if let Some(headers) = args.get("headers").and_then(|v| v.as_object()) {
        for (k, v) in headers {
            if let Some(val) = v.as_str() {
                cmd.arg("-H").arg(format!("{k}: {val}"));
            }
        }
    }
    match cmd.output() {
        Ok(out) => {
            let full = String::from_utf8_lossy(&out.stdout);
            let status = if let Some(s) = full
                .split("__STATUS__")
                .nth(1)
                .and_then(|p| p.split("__END__").next())
            {
                s.to_string()
            } else {
                "0".into()
            };
            let body = full.split("__STATUS__").next().unwrap_or("").to_string();
            let truncated = body.len() > 50000;
            ToolCallResult::ok(
                json!({
                    "status": status.parse::<u16>().unwrap_or(0),
                    "body": if truncated { &body[..50000] } else { &body },
                    "size_bytes": body.len(),
                    "truncated": truncated,
                })
                .to_string(),
            )
        }
        Err(e) => ToolCallResult::error(format!("curl: {e}")),
    }
}

fn handle_app(args: &Value) -> ToolCallResult {
    let app = args.get("app").and_then(|v| v.as_str()).unwrap_or("");
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("launch");
    let result = match action {
        "launch" => Command::new("open").args(["-a", app]).output(),
        "quit" => Command::new("osascript")
            .args(["-e", &format!("tell app \"{app}\" to quit")])
            .output(),
        _ => return ToolCallResult::error("action must be 'launch' or 'quit'"),
    };
    match result {
        Ok(out) if out.status.success() => {
            ToolCallResult::ok(json!({"app":app,"action":action,"ok":true}).to_string())
        }
        Ok(out) => {
            ToolCallResult::error(format!("failed: {}", String::from_utf8_lossy(&out.stderr)))
        }
        Err(e) => ToolCallResult::error(format!("exec: {e}")),
    }
}

fn handle_notify(args: &Value) -> ToolCallResult {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("axterminator");
    let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let sound = args
        .get("sound")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let script =
        format!("display notification \"{message}\" with title \"{title}\" sound name \"{sound}\"");
    match Command::new("osascript").args(["-e", &script]).output() {
        Ok(out) if out.status.success() => ToolCallResult::ok(json!({"notified":true}).to_string()),
        Ok(out) => ToolCallResult::error(format!(
            "osascript: {}",
            String::from_utf8_lossy(&out.stderr)
        )),
        Err(e) => ToolCallResult::error(format!("exec: {e}")),
    }
}
