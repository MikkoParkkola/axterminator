//! Shared MCP catalog helpers used by runtime summaries, tests, and guides.

use std::fmt::Write as _;

use crate::mcp::{protocol::Tool, security::SecurityMode};

const SECTION_CONNECTION: &str = "Connection";
const SECTION_FINDING: &str = "Finding Elements";
const SECTION_INTERACTION: &str = "Interaction";
const SECTION_VERIFICATION: &str = "Verification & Analysis";
const SECTION_WORKFLOW: &str = "Workflow & System";
const SECTION_AUDIO: &str = "Audio";
const SECTION_CAMERA: &str = "Camera";
const SECTION_WATCH: &str = "Watch";
const SECTION_BROWSER: &str = "Browser Containers";
const SECTION_SPACES: &str = "Virtual Desktops";
const SECTION_OTHER: &str = "Other";

const QUICKSTART_SECTION_ORDER: [&str; 11] = [
    SECTION_CONNECTION,
    SECTION_FINDING,
    SECTION_INTERACTION,
    SECTION_VERIFICATION,
    SECTION_WORKFLOW,
    SECTION_AUDIO,
    SECTION_CAMERA,
    SECTION_WATCH,
    SECTION_BROWSER,
    SECTION_SPACES,
    SECTION_OTHER,
];

const QUICKSTART_SUFFIX: &str = r#"
---

## Core Workflow Pattern

Every automation follows the same four-phase loop:

```text
ax_connect  ->  ax_find  ->  ax_click / ax_type / ax_get_value  ->  ax_assert / ax_get_value
 CONNECT         FIND              ACT                                  VERIFY
```

1. **Connect** — identify the target application with `ax_connect`.
2. **Find** — locate the element of interest with `ax_find` (or `ax_find_visual`
   when the accessibility tree is sparse).
3. **Act** — perform the interaction: click, type, set a value, press a key.
4. **Verify** — confirm the expected outcome with `ax_get_value`, `ax_assert`,
   or `ax_screenshot`.

---

## Key Concepts

### Accessibility Tree
macOS exposes every application's UI as a tree of **AXUIElement** nodes.
Each node has a *role*, optional *title*/*label*, and a set of typed *attributes*.
`ax_get_tree` dumps this structure; `ax_find` searches it.

### Roles
A role describes the semantic type of an element.
Common roles: `AXButton`, `AXTextField`, `AXStaticText`, `AXList`,
`AXTable`, `AXGroup`, `AXWindow`, `AXCheckBox`, `AXRadioButton`,
`AXComboBox`, `AXMenuItem`, `AXToolbar`.

### Attributes
Each element exposes named attributes, for example:
- `AXTitle` — visible label
- `AXValue` — current value (text, toggle state, slider position)
- `AXEnabled` — whether the element accepts interaction
- `AXFocused` — whether the element currently has keyboard focus
- `AXFrame` — bounding rectangle in screen coordinates

`ax_get_attributes` returns all attributes for a given element reference.

### AXUIElement
An `AXUIElement` is the opaque handle used by the macOS Accessibility API
to refer to a specific UI node. axterminator tools accept element handles
returned by `ax_find` and pass them to interaction tools
(`ax_click`, `ax_type`, `ax_get_value`, etc.).
"#;

#[must_use]
pub(crate) fn runtime_tools_for_mode(mode: SecurityMode) -> Vec<Tool> {
    crate::mcp::tools::all_tools()
        .into_iter()
        .filter(|tool| mode.allows_tool_descriptor(tool))
        .collect()
}

#[must_use]
pub(crate) fn tool_count_for_mode(mode: SecurityMode) -> usize {
    runtime_tools_for_mode(mode).len()
}

fn quickstart_section(tool_name: &str) -> &'static str {
    match tool_name {
        "ax_is_accessible" | "ax_connect" => SECTION_CONNECTION,
        "ax_find" | "ax_find_visual" | "ax_list_windows" | "ax_list_apps" | "ax_get_tree" => {
            SECTION_FINDING
        }
        "ax_click" | "ax_click_at" | "ax_type" | "ax_set_value" | "ax_get_value"
        | "ax_get_attributes" | "ax_key_press" | "ax_scroll" | "ax_drag" | "ax_screenshot"
        | "ax_wait_idle" => SECTION_INTERACTION,
        "ax_assert" | "ax_test_run" | "ax_analyze" | "ax_visual_diff" | "ax_a11y_audit" => {
            SECTION_VERIFICATION
        }
        "ax_query" | "ax_workflow_create" | "ax_workflow_step" | "ax_workflow_status"
        | "ax_track_workflow" | "ax_record" | "ax_undo" | "ax_run_script" | "ax_clipboard"
        | "ax_session_info" | "ax_app_profile" | "ax_system_context" | "ax_location" => {
            SECTION_WORKFLOW
        }
        "ax_audio_devices"
        | "ax_listen"
        | "ax_speak"
        | "ax_start_capture"
        | "ax_stop_capture"
        | "ax_get_transcription"
        | "ax_capture_status" => SECTION_AUDIO,
        "ax_camera_capture" | "ax_gesture_detect" | "ax_gesture_listen" => SECTION_CAMERA,
        "ax_watch_start" | "ax_watch_stop" | "ax_watch_status" => SECTION_WATCH,
        "ax_browser_launch" | "ax_browser_stop" => SECTION_BROWSER,
        "ax_list_spaces" | "ax_create_space" | "ax_move_to_space" | "ax_switch_space"
        | "ax_destroy_space" => SECTION_SPACES,
        _ => SECTION_OTHER,
    }
}

fn enabled_optional_tool_families() -> Vec<&'static str> {
    [
        cfg!(feature = "audio").then_some("audio"),
        cfg!(feature = "camera").then_some("camera"),
        cfg!(feature = "watch").then_some("watch"),
        cfg!(feature = "spaces").then_some("spaces"),
        cfg!(feature = "docker").then_some("docker"),
        cfg!(feature = "context").then_some("context"),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn escape_markdown_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn section_tools<'a>(tools: &'a [Tool], section: &str) -> Vec<&'a Tool> {
    tools
        .iter()
        .filter(|tool| quickstart_section(tool.name) == section)
        .collect()
}

#[must_use]
pub(crate) fn quickstart_markdown() -> String {
    let tools = crate::mcp::tools::all_tools();
    let mut out = String::new();

    writeln!(&mut out, "# axterminator Quickstart").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(
        &mut out,
        "axterminator is a macOS accessibility automation server that exposes the currently enabled MCP tool surface for this build. Call `tools/list` for the exact machine-readable schema."
    )
    .unwrap();
    writeln!(&mut out).unwrap();
    writeln!(
        &mut out,
        "**This build currently exposes {} MCP tools.**",
        tools.len()
    )
    .unwrap();

    let families = enabled_optional_tool_families();
    if families.is_empty() {
        writeln!(
            &mut out,
            "No optional tool families are enabled in this build. Audio, camera, watch, spaces, docker, and context tools remain feature-gated."
        )
        .unwrap();
    } else {
        writeln!(
            &mut out,
            "Enabled optional tool families in this build: {}.",
            families.join(", ")
        )
        .unwrap();
    }
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "---").unwrap();
    writeln!(&mut out).unwrap();
    writeln!(&mut out, "## Tool Catalogue").unwrap();
    writeln!(&mut out).unwrap();

    for section in QUICKSTART_SECTION_ORDER {
        let section_tools = section_tools(&tools, section);
        if section_tools.is_empty() {
            continue;
        }
        writeln!(&mut out, "### {section}").unwrap();
        writeln!(&mut out, "| Tool | Summary |").unwrap();
        writeln!(&mut out, "|------|---------|").unwrap();
        for tool in section_tools {
            writeln!(
                &mut out,
                "| `{}` | {} |",
                tool.name,
                escape_markdown_cell(tool.title)
            )
            .unwrap();
        }
        writeln!(&mut out).unwrap();
    }

    out.push_str(QUICKSTART_SUFFIX);
    out
}

#[cfg(test)]
mod tests {
    use super::{
        quickstart_markdown, quickstart_section, runtime_tools_for_mode, tool_count_for_mode,
        SECTION_OTHER,
    };
    use crate::mcp::security::SecurityMode;

    #[test]
    fn quickstart_reports_runtime_tool_count() {
        let markdown = quickstart_markdown();
        assert!(markdown.contains(&format!(
            "**This build currently exposes {} MCP tools.**",
            crate::mcp::tools::all_tools().len()
        )));
    }

    #[test]
    fn filtered_tool_count_matches_filtered_runtime_tools() {
        assert_eq!(
            tool_count_for_mode(SecurityMode::Sandboxed),
            runtime_tools_for_mode(SecurityMode::Sandboxed).len()
        );
    }

    #[test]
    fn quickstart_mentions_every_enabled_tool() {
        let markdown = quickstart_markdown();
        for tool in crate::mcp::tools::all_tools() {
            assert!(
                markdown.contains(&format!("`{}`", tool.name)),
                "quickstart missing {}",
                tool.name
            );
        }
    }

    #[test]
    fn every_enabled_tool_has_a_quickstart_section() {
        let unmapped: Vec<&'static str> = crate::mcp::tools::all_tools()
            .into_iter()
            .filter_map(|tool| {
                (quickstart_section(tool.name) == SECTION_OTHER).then_some(tool.name)
            })
            .collect();
        assert!(
            unmapped.is_empty(),
            "unmapped quickstart tools: {unmapped:?}"
        );
    }
}
