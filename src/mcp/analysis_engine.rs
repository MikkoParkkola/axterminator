//! Pure analysis engine for `ax_analyze`.
//!
//! This module owns the internal heuristics behind MCP's accessibility
//! intelligence surface while keeping MCP tool descriptors and dispatch logic
//! in `tools_innovation.rs`. The extracted seam is intentionally pure: it only
//! depends on a `SceneGraph` snapshot and returns deterministic analysis data.

use serde_json::{json, Value};

/// Deterministic analysis result for a scene snapshot.
#[derive(Debug, Clone)]
pub(crate) struct SceneAnalysis {
    pub(crate) node_count: usize,
    pub(crate) app_state: AppState,
    pub(crate) patterns: Vec<UiPattern>,
    pub(crate) suggestions: Vec<Suggestion>,
}

impl SceneAnalysis {
    /// Render the MCP-facing payload for the requested `focus` mode.
    #[must_use]
    pub(crate) fn focused_payload(&self, focus: &str) -> Value {
        let patterns_json: Vec<Value> = self.patterns.iter().map(pattern_to_json).collect();
        let suggestions_json: Vec<Value> =
            self.suggestions.iter().map(suggestion_to_json).collect();

        match focus {
            "patterns" => json!({
                "node_count": self.node_count,
                "app_state":  self.app_state.as_str(),
                "patterns":   patterns_json,
                "suggestions": []
            }),
            "state" => json!({
                "node_count": self.node_count,
                "app_state":  self.app_state.as_str(),
                "patterns":   [],
                "suggestions": []
            }),
            "actions" => json!({
                "node_count":  self.node_count,
                "app_state":   self.app_state.as_str(),
                "patterns":    [],
                "suggestions": suggestions_json
            }),
            _ => json!({
                "node_count":  self.node_count,
                "app_state":   self.app_state.as_str(),
                "patterns":    patterns_json,
                "suggestions": suggestions_json
            }),
        }
    }
}

/// Analyze the supplied scene snapshot.
#[must_use]
pub(crate) fn analyze_scene(scene: &crate::intent::SceneGraph) -> SceneAnalysis {
    let patterns = detect_ui_patterns(scene);
    let app_state = infer_app_state(scene);
    let suggestions = suggest_actions(&patterns, app_state);

    SceneAnalysis {
        node_count: scene.len(),
        app_state,
        patterns,
        suggestions,
    }
}

/// Detected UI pattern with an associated confidence score.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UiPattern {
    pub(crate) pattern: &'static str,
    pub(crate) confidence: f64,
}

/// Inferred high-level application state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppState {
    Idle,
    Loading,
    Error,
    Modal,
    AuthRequired,
}

impl AppState {
    #[must_use]
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Loading => "loading",
            Self::Error => "error",
            Self::Modal => "modal",
            Self::AuthRequired => "auth_required",
        }
    }
}

/// A suggested next action for the agent.
#[derive(Debug, Clone)]
pub(crate) struct Suggestion {
    pub(crate) action: &'static str,
    pub(crate) tool: &'static str,
    pub(crate) query: &'static str,
}

/// Serialize a single `UiPattern` to JSON.
#[must_use]
pub(crate) fn pattern_to_json(p: &UiPattern) -> Value {
    json!({ "pattern": p.pattern, "confidence": p.confidence })
}

/// Serialize a single `Suggestion` to JSON.
#[must_use]
pub(crate) fn suggestion_to_json(s: &Suggestion) -> Value {
    json!({ "action": s.action, "tool": s.tool, "query": s.query })
}

/// A node-role predicate: returns `true` when `role` matches the target.
fn has_role(nodes: &[&crate::intent::SceneNode], role: &str) -> bool {
    nodes.iter().any(|n| n.role.as_deref() == Some(role))
}

/// Returns `true` when any node's text labels contain `needle` (case-insensitive).
fn any_label_contains(nodes: &[&crate::intent::SceneNode], needle: &str) -> bool {
    nodes.iter().any(|n| {
        n.text_labels()
            .iter()
            .any(|label| label.to_lowercase().contains(needle))
    })
}

/// Detect common UI patterns from a `SceneGraph`.
///
/// Each pattern is evaluated independently; multiple patterns may match a single scene.
/// Confidence values reflect the reliability of the heuristic — stronger structural
/// signals produce higher scores.
#[must_use]
pub(crate) fn detect_ui_patterns(scene: &crate::intent::SceneGraph) -> Vec<UiPattern> {
    let nodes: Vec<&crate::intent::SceneNode> = scene.iter().collect();
    let mut patterns = Vec::new();

    let has_password = has_role(&nodes, "AXSecureTextField");
    let has_text_field = has_role(&nodes, "AXTextField");
    let has_button = has_role(&nodes, "AXButton");
    if has_password && has_text_field && has_button {
        patterns.push(UiPattern {
            pattern: "login_form",
            confidence: 0.90,
        });
    }

    let has_search_field = has_role(&nodes, "AXSearchField");
    let has_search_label = has_text_field && any_label_contains(&nodes, "search");
    if has_search_field || has_search_label {
        patterns.push(UiPattern {
            pattern: "search_interface",
            confidence: 0.85,
        });
    }

    let has_tab_group = has_role(&nodes, "AXTabGroup");
    let has_toolbar = has_role(&nodes, "AXToolbar");
    if has_tab_group || has_toolbar {
        patterns.push(UiPattern {
            pattern: "navigation",
            confidence: 0.80,
        });
    }

    let has_table =
        has_role(&nodes, "AXTable") || has_role(&nodes, "AXGrid") || has_role(&nodes, "AXOutline");
    if has_table {
        patterns.push(UiPattern {
            pattern: "table_view",
            confidence: 0.88,
        });
    }

    let has_modal = has_role(&nodes, "AXSheet") || has_role(&nodes, "AXDialog");
    if has_modal {
        patterns.push(UiPattern {
            pattern: "modal_dialog",
            confidence: 0.95,
        });
    }

    if has_modal && has_button {
        let save_btn = any_label_contains(&nodes, "save");
        let open_btn = any_label_contains(&nodes, "open");
        let cancel_btn = any_label_contains(&nodes, "cancel");
        if save_btn && cancel_btn {
            patterns.push(UiPattern {
                pattern: "file_save_dialog",
                confidence: 0.88,
            });
        } else if open_btn && cancel_btn {
            patterns.push(UiPattern {
                pattern: "file_open_dialog",
                confidence: 0.88,
            });
        }
    }

    let has_alert = has_role(&nodes, "AXAlert");
    if has_alert && has_button {
        let ok = any_label_contains(&nodes, "ok") || any_label_contains(&nodes, "yes");
        let cancel = any_label_contains(&nodes, "cancel") || any_label_contains(&nodes, "no");
        if ok && cancel {
            patterns.push(UiPattern {
                pattern: "confirmation_dialog",
                confidence: 0.87,
            });
        } else {
            patterns.push(UiPattern {
                pattern: "error_alert",
                confidence: 0.80,
            });
        }
    }

    let has_groups = scene.nodes_by_role("AXGroup").len() >= 3;
    let has_checkboxes = has_role(&nodes, "AXCheckBox");
    let has_popups = has_role(&nodes, "AXPopUpButton");
    if has_groups && (has_checkboxes || has_popups) && !has_modal && !has_password {
        patterns.push(UiPattern {
            pattern: "settings_page",
            confidence: 0.75,
        });
    }

    let has_text_area = has_role(&nodes, "AXTextArea");
    if has_text_area && (has_toolbar || nodes.len() > 10) {
        patterns.push(UiPattern {
            pattern: "text_editor",
            confidence: 0.78,
        });
    }

    let browser_addr = nodes.iter().any(|node| {
        node.role.as_deref() == Some("AXTextField")
            && node.identifier.as_deref().is_some_and(|identifier| {
                identifier.contains("address") || identifier.contains("url")
            })
    });
    if browser_addr && has_tab_group {
        patterns.push(UiPattern {
            pattern: "browser_main",
            confidence: 0.85,
        });
    }

    let text_field_count = scene.nodes_by_role("AXTextField").len();
    if text_field_count >= 2 && !has_password && has_button {
        patterns.push(UiPattern {
            pattern: "form",
            confidence: 0.72,
        });
    }

    let has_progress =
        has_role(&nodes, "AXProgressIndicator") || has_role(&nodes, "AXBusyIndicator");
    if has_progress {
        patterns.push(UiPattern {
            pattern: "progress_indicator",
            confidence: 0.93,
        });
    }

    patterns
}

/// Infer the high-level application state from a `SceneGraph`.
///
/// States are evaluated in priority order: modal > loading > error > auth_required > idle.
#[must_use]
pub(crate) fn infer_app_state(scene: &crate::intent::SceneGraph) -> AppState {
    let nodes: Vec<&crate::intent::SceneNode> = scene.iter().collect();

    if has_role(&nodes, "AXSheet") || has_role(&nodes, "AXDialog") {
        return AppState::Modal;
    }

    let loading = has_role(&nodes, "AXProgressIndicator")
        || has_role(&nodes, "AXBusyIndicator")
        || any_label_contains(&nodes, "loading");
    if loading {
        return AppState::Loading;
    }

    let error = has_role(&nodes, "AXAlert")
        || any_label_contains(&nodes, "error")
        || any_label_contains(&nodes, "failed")
        || any_label_contains(&nodes, "invalid");
    if error {
        return AppState::Error;
    }

    if has_role(&nodes, "AXSecureTextField") {
        return AppState::AuthRequired;
    }

    AppState::Idle
}

/// Generate next-action suggestions from detected patterns and app state.
///
/// Suggestions are purely informational — they are never executed automatically.
/// The list is ordered from most-specific to most-general.
#[must_use]
pub(crate) fn suggest_actions(patterns: &[UiPattern], state: AppState) -> Vec<Suggestion> {
    let mut suggestions: Vec<Suggestion> = Vec::new();

    // State-driven suggestions take priority.
    match state {
        AppState::Modal => {
            suggestions.push(Suggestion {
                action: "Dismiss or interact with the modal dialog before continuing",
                tool: "ax_click",
                query: "Cancel",
            });
        }
        AppState::Loading => {
            suggestions.push(Suggestion {
                action: "Wait for the app to finish loading",
                tool: "ax_wait_idle",
                query: "",
            });
        }
        AppState::Error => {
            suggestions.push(Suggestion {
                action: "Acknowledge the error and check error details",
                tool: "ax_get_value",
                query: "error message",
            });
        }
        AppState::AuthRequired => {
            suggestions.push(Suggestion {
                action: "Enter credentials to authenticate",
                tool: "ax_type",
                query: "username",
            });
        }
        AppState::Idle => {}
    }

    // Pattern-driven suggestions.
    let pattern_names: Vec<&str> = patterns.iter().map(|p| p.pattern).collect();

    if pattern_names.contains(&"login_form") {
        suggestions.push(Suggestion {
            action: "Type your username into the text field",
            tool: "ax_type",
            query: "username",
        });
        suggestions.push(Suggestion {
            action: "Type your password into the secure field",
            tool: "ax_type",
            query: "password",
        });
        suggestions.push(Suggestion {
            action: "Click the sign-in button to submit credentials",
            tool: "ax_click",
            query: "Sign In",
        });
    }

    if pattern_names.contains(&"search_interface") {
        suggestions.push(Suggestion {
            action: "Type your query into the search field",
            tool: "ax_type",
            query: "search",
        });
    }

    if pattern_names.contains(&"file_save_dialog") {
        suggestions.push(Suggestion {
            action: "Type a filename and click Save to confirm",
            tool: "ax_type",
            query: "Save As",
        });
        suggestions.push(Suggestion {
            action: "Click Save to confirm the file",
            tool: "ax_click",
            query: "Save",
        });
    }

    if pattern_names.contains(&"file_open_dialog") {
        suggestions.push(Suggestion {
            action: "Navigate to the desired file and click Open",
            tool: "ax_click",
            query: "Open",
        });
    }

    if pattern_names.contains(&"confirmation_dialog") {
        suggestions.push(Suggestion {
            action: "Confirm the action by clicking OK or Yes",
            tool: "ax_click",
            query: "OK",
        });
        suggestions.push(Suggestion {
            action: "Cancel the action to dismiss the dialog",
            tool: "ax_click",
            query: "Cancel",
        });
    }

    if pattern_names.contains(&"error_alert") {
        suggestions.push(Suggestion {
            action: "Dismiss the error alert",
            tool: "ax_click",
            query: "OK",
        });
    }

    if pattern_names.contains(&"table_view") {
        suggestions.push(Suggestion {
            action: "Read the visible rows from the data table",
            tool: "ax_get_value",
            query: "table row",
        });
    }

    if pattern_names.contains(&"text_editor") {
        suggestions.push(Suggestion {
            action: "Type or edit text in the editor area",
            tool: "ax_type",
            query: "text area",
        });
    }

    if pattern_names.contains(&"form") {
        suggestions.push(Suggestion {
            action: "Fill in the form fields",
            tool: "ax_type",
            query: "text field",
        });
        suggestions.push(Suggestion {
            action: "Submit the form",
            tool: "ax_click",
            query: "Submit",
        });
    }

    suggestions
}
