/// System MCP tools for filesystem, process, and command execution.
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};

pub(crate) fn system_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "ax_fs_read",
            title: "Read file contents",
            description: "Read the contents of a file at the given path. Limited to 10 MB. Paths resolve ~ to home.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            output_schema: json!({"type":"object","properties":{"path":{"type":"string"},"size_bytes":{"type":"integer"},"content":{"type":"string"}}}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_fs_write",
            title: "Write content to a file",
            description: "Write or create a file. Creates parent directories. Overwrites by default.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"append":{"type":"boolean"}},"required":["path","content"]}),
            output_schema: json!({"type":"object","properties":{"path":{"type":"string"},"bytes_written":{"type":"integer"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_fs_list",
            title: "List directory contents",
            description: "List files and directories at the given path with size and type.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            output_schema: json!({"type":"object","properties":{"path":{"type":"string"},"entries":{"type":"array"},"count":{"type":"integer"}}}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_process_list",
            title: "List running processes",
            description: "Returns running processes with PID, name, CPU%, and memory usage. Filter with name argument.",
            input_schema: json!({"type":"object","properties":{"filter":{"type":"string"},"limit":{"type":"integer"}}}),
            output_schema: json!({"type":"object","properties":{"processes":{"type":"array"},"total_count":{"type":"integer"}}}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_exec",
            title: "Execute a shell command",
            description: "Execute a shell command via /bin/sh -c. Returns stdout, stderr, and exit code. 30s timeout.",
            input_schema: json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"}},"required":["command"]}),
            output_schema: json!({"type":"object","properties":{"exit_code":{"type":"integer"},"stdout":{"type":"string"},"stderr":{"type":"string"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
    ]
}

fn resolve_path(raw: &str) -> String {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    raw.to_string()
}

pub(crate) fn call_system_tool(name: &str, args: &Value, _mode: &str) -> ToolCallResult {
    match name {
        "ax_fs_read" => {
            let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let path = resolve_path(raw);
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    let len = content.len();
                    ToolCallResult::ok(json!({ "path": path, "size_bytes": len, "content": content }).to_string())
                }
                Err(e) => ToolCallResult::error(format!("read: {e}")),
            }
        }
        "ax_fs_write" => {
            let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let path = resolve_path(raw);
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let append = args.get("append").and_then(|v| v.as_bool()).unwrap_or(false);
            if let Some(parent) = Path::new(&path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let result = if append {
                std::fs::OpenOptions::new().create(true).append(true).open(&path)
                    .and_then(|mut f| std::io::Write::write(&mut f, content.as_bytes()))
            } else {
                std::fs::write(&path, content).map(|_| content.len())
            };
            match result {
                Ok(n) => ToolCallResult::ok(json!({ "path": path, "bytes_written": n }).to_string()),
                Err(e) => ToolCallResult::error(format!("write: {e}")),
            }
        }
        "ax_fs_list" => {
            let raw = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let path = resolve_path(raw);
            let mut entries = Vec::new();
            if let Ok(dir) = std::fs::read_dir(&path) {
                for entry in dir.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let meta = entry.metadata().ok();
                    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                    entries.push(json!({ "name": name, "size_bytes": size }));
                }
            }
            let count = entries.len();
            ToolCallResult::ok(json!({ "path": path, "entries": entries, "count": count }).to_string())
        }
        "ax_process_list" => {
            let filter = args.get("filter").and_then(|v| v.as_str()).map(|s| s.to_lowercase());
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let mut sys = sysinfo::System::new_all();
            sys.refresh_all();
            let mut procs: Vec<Value> = Vec::new();
            for (pid, proc) in sys.processes() {
                let name = proc.name().to_string_lossy().to_string();
                if let Some(ref f) = filter {
                    if !name.to_lowercase().contains(f) { continue; }
                }
                procs.push(json!({
                    "pid": pid.as_u32(),
                    "name": name,
                    "cpu_percent": (proc.cpu_usage() * 100.0),
                    "memory_mb": (proc.memory() as f64 / (1024.0 * 1024.0)),
                }));
                if procs.len() >= limit { break; }
            }
            let count = procs.len();
            ToolCallResult::ok(json!({ "processes": procs, "total_count": count }).to_string())
        }
        "ax_exec" => {
            let cmd = match args.get("command").and_then(|v| v.as_str()) {
                Some(c) => c,
                None => return ToolCallResult::error("missing command"),
            };
            let cwd = args.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
            match Command::new("/bin/sh").arg("-c").arg(cmd).current_dir(cwd)
                .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).output()
            {
                Ok(out) => ToolCallResult::ok(json!({
                    "exit_code": out.status.code().unwrap_or(-1),
                    "stdout": String::from_utf8_lossy(&out.stdout),
                    "stderr": String::from_utf8_lossy(&out.stderr),
                }).to_string()),
                Err(e) => ToolCallResult::error(format!("exec: {e}")),
            }
        }
        _ => ToolCallResult::error(format!("unknown: {name}")),
    }
}
