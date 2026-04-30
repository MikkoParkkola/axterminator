//! Window management MCP tools — move, resize, focus, minimize macOS windows.
//! Uses CoreGraphics and Accessibility APIs for window control.
use serde_json::{json, Value};
use std::process::Command;

use crate::mcp::annotations;
use crate::mcp::protocol::{Tool, ToolCallResult};

pub(crate) fn window_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "ax_window_list",
            title: "List all windows with positions",
            description: "Returns all visible windows across all apps with title, position, size, and app name.",
            input_schema: json!({"type":"object","properties":{"app":{"type":"string","description":"Optional app filter by name"}},"required":[]}),
            output_schema: json!({"type":"object","properties":{"windows":{"type":"array"},"count":{"type":"integer"}}}),
            annotations: annotations::READ_ONLY,
        },
        Tool {
            name: "ax_window_focus",
            title: "Focus a window by title or app",
            description: "Bring a window to the foreground. Uses osascript to activate the app and raise the window.",
            input_schema: json!({"type":"object","properties":{"app":{"type":"string","description":"App name to activate"},"title":{"type":"string","description":"Window title to focus (optional)"}},"required":["app"]}),
            output_schema: json!({"type":"object","properties":{"focused":{"type":"string"},"ok":{"type":"boolean"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_window_move",
            title: "Move a window to new coordinates",
            description: "Move a window by title to new x,y coordinates. Uses AppleScript to set position.",
            input_schema: json!({"type":"object","properties":{"app":{"type":"string","description":"App name"},"title":{"type":"string","description":"Window title (partial match)"},"x":{"type":"integer"},"y":{"type":"integer"}},"required":["app","x","y"]}),
            output_schema: json!({"type":"object","properties":{"moved":{"type":"boolean"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_window_resize",
            title: "Resize a window",
            description: "Resize a window by title to new width and height. Uses AppleScript bounds.",
            input_schema: json!({"type":"object","properties":{"app":{"type":"string","description":"App name"},"title":{"type":"string","description":"Window title (partial match)"},"width":{"type":"integer"},"height":{"type":"integer"}},"required":["app","width","height"]}),
            output_schema: json!({"type":"object","properties":{"resized":{"type":"boolean"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_window_minimize",
            title: "Minimize a window",
            description: "Minimize a window by title or minimize all windows of an app.",
            input_schema: json!({"type":"object","properties":{"app":{"type":"string","description":"App name"},"title":{"type":"string","description":"Window title (optional, minimizes all if omitted)"}},"required":["app"]}),
            output_schema: json!({"type":"object","properties":{"minimized":{"type":"boolean"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
        Tool {
            name: "ax_window_tile",
            title: "Tile a window to screen region",
            description: "Tile a window to left, right, top, bottom, or full screen. Uses built-in macOS window tiling (macOS 15+).",
            input_schema: json!({"type":"object","properties":{"app":{"type":"string","description":"App name"},"position":{"type":"string","description":"left, right, top, bottom, or full"}},"required":["app","position"]}),
            output_schema: json!({"type":"object","properties":{"tiled":{"type":"boolean"},"position":{"type":"string"}}}),
            annotations: annotations::DESTRUCTIVE,
        },
    ]
}

pub(crate) fn call_window_tool(name: &str, args: &Value, _mode: &str) -> ToolCallResult {
    match name {
        "ax_window_list" => handle_window_list(args),
        "ax_window_focus" => handle_window_focus(args),
        "ax_window_move" => handle_window_move(args),
        "ax_window_resize" => handle_window_resize(args),
        "ax_window_minimize" => handle_window_minimize(args),
        "ax_window_tile" => handle_window_tile(args),
        _ => ToolCallResult::error(format!("unknown: {name}")),
    }
}

fn handle_window_list(args: &Value) -> ToolCallResult {
    let filter = args
        .get("app")
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase());

    // Use a combined approach: osascript for standard windows + CGWindowList for all
    let script = r#"
        tell application "System Events"
            set output to ""
            repeat with p in (every process whose background only is false)
                set pname to name of p
                try
                    repeat with w in windows of p
                        set output to output & pname & "|" & name of w & "|" & 
                            (position of w as text) & "|" & (size of w as text) & linefeed
                    end repeat
                end try
            end repeat
            return output
        end tell
    "#;

    let mut windows = Vec::new();
    if let Ok(out) = Command::new("osascript").args(["-e", script]).output() {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() >= 4 {
                let app = parts[0].to_string();
                let title = parts[1].to_string();
                if let Some(ref f) = filter {
                    if !app.to_lowercase().contains(f) {
                        continue;
                    }
                }
                windows.push(
                    json!({"app": app, "title": title, "bounds": parts[2], "size": parts[3]}),
                );
            }
        }
    }

    ToolCallResult::ok(json!({"windows": windows, "count": windows.len()}).to_string())
}

fn handle_window_focus(args: &Value) -> ToolCallResult {
    let app = args.get("app").and_then(|v| v.as_str()).unwrap_or("");
    let title = args.get("title").and_then(|v| v.as_str());

    let script = if let Some(t) = title {
        let escaped = t.replace('"', "\\\"");
        format!(
            r#"tell app "{app}" to activate
tell application "System Events"
    tell process "{app}"
        set frontmost to true
        repeat with w in windows
            if name of w contains "{escaped}" then
                set index of w to 1
                exit repeat
            end if
        end repeat
    end tell
end tell"#
        )
    } else {
        format!(r#"tell app "{app}" to activate"#)
    };

    match Command::new("osascript").args(["-e", &script]).output() {
        Ok(out) if out.status.success() => {
            ToolCallResult::ok(json!({"focused": app, "ok": true}).to_string())
        }
        Ok(_) => ToolCallResult::error(format!("could not focus: {app}")),
        Err(e) => ToolCallResult::error(format!("osascript: {e}")),
    }
}

fn handle_window_move(args: &Value) -> ToolCallResult {
    let app = args.get("app").and_then(|v| v.as_str()).unwrap_or("");
    let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let x = args.get("x").and_then(|v| v.as_i64()).unwrap_or(0);
    let y = args.get("y").and_then(|v| v.as_i64()).unwrap_or(0);

    let script = format!(
        r#"tell application "System Events"
    tell process "{app}"
        repeat with w in windows
            if name of w contains "{title}" then
                set position of w to {{{x}, {y}}}
                exit repeat
            end if
        end repeat
    end tell
end tell"#
    );

    run_script(&script, "move")
}

fn handle_window_resize(args: &Value) -> ToolCallResult {
    let app = args.get("app").and_then(|v| v.as_str()).unwrap_or("");
    let title = args.get("title").and_then(|v| v.as_str()).unwrap_or("");
    let w = args.get("width").and_then(|v| v.as_i64()).unwrap_or(800);
    let h = args.get("height").and_then(|v| v.as_i64()).unwrap_or(600);

    let script = format!(
        r#"tell application "System Events"
    tell process "{app}"
        repeat with w in windows
            if name of w contains "{title}" then
                set size of w to {{{w}, {h}}}
                exit repeat
            end if
        end repeat
    end tell
end tell"#
    );

    run_script(&script, "resize")
}

fn handle_window_minimize(args: &Value) -> ToolCallResult {
    let app = args.get("app").and_then(|v| v.as_str()).unwrap_or("");
    let title = args.get("title").and_then(|v| v.as_str());

    let script = if let Some(t) = title {
        format!(
            r#"tell application "System Events"
    tell process "{app}"
        repeat with w in windows
            if name of w contains "{t}" then
                set miniaturized of w to true
                exit repeat
            end if
        end repeat
    end tell
end tell"#
        )
    } else {
        format!(
            r#"tell application "System Events"
    tell process "{app}"
        repeat with w in windows
            set miniaturized of w to true
        end repeat
    end tell
end tell"#
        )
    };

    run_script(&script, "minimize")
}

fn handle_window_tile(args: &Value) -> ToolCallResult {
    let app = args.get("app").and_then(|v| v.as_str()).unwrap_or("");
    let position = args
        .get("position")
        .and_then(|v| v.as_str())
        .unwrap_or("full");

    // Activate app then send keyboard shortcut for tiling
    let shortcut = match position {
        "left" => "124",  // Ctrl+Cmd+Right (actually use built-in window menu)
        "right" => "123", // Ctrl+Cmd+Left
        "full" => "70",   // Ctrl+Cmd+F
        _ => return ToolCallResult::error("position must be left, right, or full"),
    };

    // Activate first
    let _ = Command::new("osascript")
        .args(["-e", &format!(r#"tell app "{app}" to activate"#)])
        .output();
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Use the Window menu: Window > Tile Window to Left/Right, or Window > Zoom
    let menu_item = match position {
        "left" => "Tile Window to Left of Screen",
        "right" => "Tile Window to Right of Screen",
        "full" => "Zoom",
        _ => "",
    };

    let script = format!(
        r#"tell application "System Events"
    tell process "{app}"
        click menu item "{menu_item}" of menu "Window" of menu bar 1
    end tell
end tell"#
    );

    run_script(&script, "tile")
}

fn run_script(script: &str, action: &str) -> ToolCallResult {
    match Command::new("osascript").args(["-e", script]).output() {
        Ok(out) if out.status.success() => {
            ToolCallResult::ok(json!({format!("{action}d"): true}).to_string())
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr).to_string();
            ToolCallResult::ok(json!({format!("{action}d"): false, "error": err}).to_string())
        }
        Err(e) => ToolCallResult::error(format!("osascript: {e}")),
    }
}
