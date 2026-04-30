//! Extended system MCP tools: memory, disk, network, power, launchd.
use serde_json::{json, Value};
use std::process::Command;

use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};

pub(crate) fn extended_system_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "ax_system_memory",
            title: "Get memory statistics",
            description: "Physical memory, swap, memory pressure, top consumers.",
            input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_system_disk",
            title: "Get disk usage",
            description: "Free space and capacity for a path via df.",
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}}}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_system_network",
            title: "Get network interfaces",
            description: "Network interfaces with IPs and MAC addresses.",
            input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_system_power",
            title: "Get power and thermal status",
            description: "Battery, charging, thermal, CPU, uptime via pmset + sysctl.",
            input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_system_launchd",
            title: "List launchd agents",
            description: "User launchd agents with load status.",
            input_schema: json!({"type":"object","properties":{"filter":{"type":"string"}}}),
            output_schema: json!({"type":"object"}),
            annotations: annotations::READ_ONLY,
        },
    ]
}

pub(crate) fn call_extended_system_tool(name: &str, args: &Value, _mode: &str) -> ToolCallResult {
    match name {
        "ax_system_memory" => handle_memory(),
        "ax_system_disk" => handle_disk(args),
        "ax_system_network" => handle_network(),
        "ax_system_power" => handle_power(),
        "ax_system_launchd" => handle_launchd(args),
        _ => ToolCallResult::error(format!("unknown: {name}")),
    }
}

fn handle_memory() -> ToolCallResult {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let total = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let used = sys.used_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let pressure = memory_pressure();
    ToolCallResult::ok(json!({
        "total_gb": (total * 10.0).round() / 10.0,
        "used_gb": (used * 10.0).round() / 10.0,
        "swap_total_gb": (sys.total_swap() as f64 / (1024.0*1024.0*1024.0) * 10.0).round() / 10.0,
        "swap_used_gb": (sys.used_swap() as f64 / (1024.0*1024.0*1024.0) * 10.0).round() / 10.0,
        "memory_pressure": pressure,
    }).to_string())
}

fn memory_pressure() -> String {
    if let Ok(out) = Command::new("sysctl")
        .args(["-n", "vm.memory_pressure_level"])
        .output()
    {
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        match v.as_str() {
            "1" => "normal",
            "2" => "warn",
            "4" => "critical",
            _ => &v,
        }
        .to_string()
    } else {
        "unknown".into()
    }
}

fn handle_disk(args: &Value) -> ToolCallResult {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("/");
    if let Ok(out) = Command::new("df").args(["-k", path]).output() {
        if let Some(line) = String::from_utf8_lossy(&out.stdout).lines().nth(1) {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() >= 4 {
                if let (Ok(blk), Ok(avail)) = (p[1].parse::<f64>(), p[3].parse::<f64>()) {
                    let tg = blk / (1024.0 * 1024.0);
                    let fg = avail / (1024.0 * 1024.0);
                    return ToolCallResult::ok(
                        json!({
                            "total_gb": (tg * 10.0).round() / 10.0,
                            "free_gb": (fg * 10.0).round() / 10.0,
                            "used_gb": ((tg - fg) * 10.0).round() / 10.0,
                        })
                        .to_string(),
                    );
                }
            }
        }
    }
    ToolCallResult::error("df failed")
}

fn handle_network() -> ToolCallResult {
    let nets = sysinfo::Networks::new_with_refreshed_list();
    let ifaces: Vec<Value> = nets
        .iter()
        .map(|(n, net)| {
            json!({
                "name": n,
                "mac": net.mac_address().to_string(),
                "ips": net.ip_networks().iter().map(|ip| ip.addr.to_string()).collect::<Vec<_>>(),
            })
        })
        .collect();
    ToolCallResult::ok(json!({ "interfaces": ifaces }).to_string())
}

fn handle_power() -> ToolCallResult {
    let (bat, chg, src) = {
        let mut b: Option<f64> = None;
        let mut c = None;
        let mut s: Option<String> = None;
        if let Ok(out) = Command::new("pmset").args(["-g", "batt"]).output() {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Some(pct) = line.split_whitespace().find(|w| w.ends_with('%')) {
                    b = pct.trim_end_matches('%').parse::<f64>().ok();
                }
                if line.contains("charging") {
                    c = Some(true);
                }
                if line.contains("discharging") {
                    c = Some(false);
                }
                if line.contains("AC Power") {
                    s = Some("ac".into());
                }
                if line.contains("Battery Power") {
                    s = Some("battery".into());
                }
            }
        }
        (b, c, s)
    };
    let mut sys = sysinfo::System::new_all();
    sys.refresh_cpu_all();
    std::thread::sleep(std::time::Duration::from_millis(50));
    sys.refresh_cpu_all();
    let cpu = sys.global_cpu_usage() * 100.0;
    let thermal = if let Ok(out) = Command::new("sysctl")
        .args(["-n", "machdep.xcpm.cpu_thermal_level"])
        .output()
    {
        match String::from_utf8_lossy(&out.stdout).trim() {
            "0" | "" => "nominal",
            s => s,
        }
        .to_string()
    } else {
        "unknown".into()
    };
    ToolCallResult::ok(
        json!({
            "battery_pct": bat,
            "charging": chg,
            "power_source": src,
            "thermal_state": thermal,
            "cpu_usage_pct": (cpu * 10.0).round() / 10.0,
            "uptime_secs": sysinfo::System::uptime(),
        })
        .to_string(),
    )
}

fn handle_launchd(args: &Value) -> ToolCallResult {
    let filter = args
        .get("filter")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());
    let dir = format!(
        "{}/Library/LaunchAgents",
        std::env::var("HOME").unwrap_or_default()
    );
    let mut agents = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let loaded_list = String::from_utf8_lossy(
            &Command::new("launchctl")
                .args(["list"])
                .output()
                .map(|o| o.stdout)
                .unwrap_or_default(),
        )
        .to_string();
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(ref f) = filter {
                if !name.to_lowercase().contains(f) {
                    continue;
                }
            }
            agents.push(json!({ "name": name, "loaded": loaded_list.contains(&name), "path": format!("{dir}/{name}") }));
        }
    }
    ToolCallResult::ok(json!({ "agents": agents, "total_count": agents.len() }).to_string())
}
