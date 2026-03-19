//! LLM-optimised formatting of [`CopilotState`] snapshots.
//!
//! Provides two output representations:
//!
//! * **Plain text** (`format_for_llm`) — terse, token-efficient prose a model
//!   can drop directly into its system prompt.
//! * **JSON** (`format_as_json`) — structured `serde_json::Value` for
//!   programmatic consumers or tool-call arguments.
//!
//! Both paths respect a configurable token budget so the output never exceeds
//! a target context window.
//!
//! # Example
//!
//! ```
//! use axterminator::copilot_state::CopilotState;
//! use axterminator::copilot_format::{format_for_llm, format_as_json, FormatOptions};
//!
//! let state = CopilotState::empty();
//! let opts = FormatOptions::default();
//!
//! let text = format_for_llm(&state, &opts);
//! assert!(!text.is_empty());
//!
//! let json = format_as_json(&state, &opts);
//! assert!(json.is_object());
//! ```

use serde_json::{json, Value};

use crate::copilot_state::{CopilotState, StateChange};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Token budget and formatting knobs.
///
/// The default budget (2 048 tokens) suits most chat model context windows
/// where the state is one of many messages.
#[derive(Debug, Clone)]
pub struct FormatOptions {
    /// Approximate character limit for the plain-text output.
    ///
    /// One token ≈ 4 characters (GPT-family heuristic); the default 2 048
    /// token budget therefore maps to ~8 192 characters.
    pub char_budget: usize,
    /// Include the Unix timestamp in text output.
    pub include_timestamp: bool,
    /// Prefix prepended to every plain-text block.
    pub prefix: Option<String>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            char_budget: 8_192,
            include_timestamp: false,
            prefix: None,
        }
    }
}

impl FormatOptions {
    /// Convenience constructor with explicit token budget.
    ///
    /// `token_budget` is converted to a character budget using the 1:4 ratio.
    #[must_use]
    pub fn with_token_budget(tokens: usize) -> Self {
        Self {
            char_budget: tokens * 4,
            ..Self::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Plain-text formatter
// ---------------------------------------------------------------------------

/// Format a [`CopilotState`] as concise text for direct LLM context injection.
///
/// Output is structured as labelled key-value lines, omitting `None` / empty
/// values. The total length is capped at `opts.char_budget`.
///
/// # Example
///
/// ```
/// use axterminator::copilot_state::CopilotState;
/// use axterminator::copilot_format::{format_for_llm, FormatOptions};
///
/// let mut state = CopilotState::empty();
/// state.app.name = Some("Finder".to_owned());
///
/// let text = format_for_llm(&state, &FormatOptions::default());
/// assert!(text.contains("Finder"));
/// ```
#[must_use]
pub fn format_for_llm(state: &CopilotState, opts: &FormatOptions) -> String {
    let mut buf = String::with_capacity(512);

    if let Some(ref prefix) = opts.prefix {
        push_line(&mut buf, prefix);
    }

    append_app_section(&mut buf, state);
    append_selection_section(&mut buf, state);
    append_navigation_section(&mut buf, state);
    append_content_section(&mut buf, state);

    if opts.include_timestamp {
        push_kv(&mut buf, "timestamp", &state.timestamp.to_string());
    }

    truncate_to_budget(buf, opts.char_budget)
}

// ---------------------------------------------------------------------------
// JSON formatter
// ---------------------------------------------------------------------------

/// Serialise a [`CopilotState`] as a `serde_json::Value` object.
///
/// Keys with `None` values are omitted to minimise token usage. The returned
/// object respects `opts.char_budget` by truncating text fields that would
/// push the serialised output over the limit.
///
/// # Example
///
/// ```
/// use axterminator::copilot_state::CopilotState;
/// use axterminator::copilot_format::{format_as_json, FormatOptions};
///
/// let state = CopilotState::empty();
/// let json = format_as_json(&state, &FormatOptions::default());
/// assert!(json["app"].is_object());
/// ```
#[must_use]
pub fn format_as_json(state: &CopilotState, opts: &FormatOptions) -> Value {
    let mut obj = json!({
        "app":       build_app_json(state),
        "selection": build_selection_json(state),
        "navigation": build_navigation_json(state),
        "content":   build_content_json(state, opts),
    });

    if opts.include_timestamp {
        obj["timestamp"] = json!(state.timestamp);
    }

    obj
}

/// Format a list of [`StateChange`] records as concise text for an LLM.
///
/// Each change is rendered as `field: old -> new`.
#[must_use]
pub fn format_changes_for_llm(changes: &[StateChange], opts: &FormatOptions) -> String {
    if changes.is_empty() {
        return "No state changes.".to_owned();
    }

    let mut buf = String::with_capacity(changes.len() * 64);
    buf.push_str("State changes:\n");

    for change in changes {
        let line = format!(
            "  {}: {} -> {}\n",
            change.field, change.old_value, change.new_value
        );
        buf.push_str(&line);
    }

    truncate_to_budget(buf, opts.char_budget)
}

// ---------------------------------------------------------------------------
// Section builders — plain text
// ---------------------------------------------------------------------------

fn append_app_section(buf: &mut String, state: &CopilotState) {
    push_section(buf, "App");
    push_opt_kv(buf, "name", &state.app.name);
    push_opt_kv(buf, "focused_window", &state.app.focused_window);
    push_opt_kv(buf, "active_tab", &state.app.active_tab);
    push_opt_kv(buf, "active_document", &state.app.active_document);
}

fn append_selection_section(buf: &mut String, state: &CopilotState) {
    let sel = &state.selection;
    if sel.selected_text.is_none()
        && sel.selected_list_row.is_none()
        && sel.selected_items.is_empty()
    {
        return;
    }
    push_section(buf, "Selection");
    push_opt_kv(buf, "text", &sel.selected_text);
    if let Some(row) = sel.selected_list_row {
        push_kv(buf, "list_row", &row.to_string());
    }
    if !sel.selected_items.is_empty() {
        push_kv(buf, "items", &sel.selected_items.join(", "));
    }
}

fn append_navigation_section(buf: &mut String, state: &CopilotState) {
    let nav = &state.navigation;
    if nav.breadcrumb.is_empty()
        && nav.sidebar_selection.is_none()
        && nav.tab_bar_selection.is_none()
    {
        return;
    }
    push_section(buf, "Navigation");
    if !nav.breadcrumb.is_empty() {
        push_kv(buf, "breadcrumb", &nav.breadcrumb.join(" > "));
    }
    push_opt_kv(buf, "sidebar", &nav.sidebar_selection);
    push_opt_kv(buf, "tab_bar", &nav.tab_bar_selection);
}

fn append_content_section(buf: &mut String, state: &CopilotState) {
    let content = &state.content;
    if content.document_title.is_none()
        && content.visible_text_excerpt.is_none()
        && content.form_fields.is_empty()
        && content.focused_element_role.is_none()
    {
        return;
    }
    push_section(buf, "Content");
    push_opt_kv(buf, "document", &content.document_title);
    push_opt_kv(buf, "focused_role", &content.focused_element_role);
    push_opt_kv(buf, "focused_title", &content.focused_element_title);
    push_opt_kv(buf, "excerpt", &content.visible_text_excerpt);
    for (label, value) in &content.form_fields {
        push_kv(buf, label, value);
    }
}

// ---------------------------------------------------------------------------
// Section builders — JSON
// ---------------------------------------------------------------------------

fn build_app_json(state: &CopilotState) -> Value {
    let mut m = serde_json::Map::new();
    insert_opt(&mut m, "name", &state.app.name);
    insert_opt(&mut m, "focused_window", &state.app.focused_window);
    insert_opt(&mut m, "active_tab", &state.app.active_tab);
    insert_opt(&mut m, "active_document", &state.app.active_document);
    Value::Object(m)
}

fn build_selection_json(state: &CopilotState) -> Value {
    let mut m = serde_json::Map::new();
    insert_opt(&mut m, "selected_text", &state.selection.selected_text);
    if let Some(row) = state.selection.selected_list_row {
        m.insert("selected_list_row".to_owned(), json!(row));
    }
    if !state.selection.selected_items.is_empty() {
        m.insert(
            "selected_items".to_owned(),
            json!(state.selection.selected_items),
        );
    }
    Value::Object(m)
}

fn build_navigation_json(state: &CopilotState) -> Value {
    let mut m = serde_json::Map::new();
    if !state.navigation.breadcrumb.is_empty() {
        m.insert("breadcrumb".to_owned(), json!(state.navigation.breadcrumb));
    }
    insert_opt(
        &mut m,
        "sidebar_selection",
        &state.navigation.sidebar_selection,
    );
    insert_opt(
        &mut m,
        "tab_bar_selection",
        &state.navigation.tab_bar_selection,
    );
    m.insert("depth".to_owned(), json!(state.navigation.depth));
    Value::Object(m)
}

fn build_content_json(state: &CopilotState, opts: &FormatOptions) -> Value {
    let mut m = serde_json::Map::new();
    insert_opt(&mut m, "document_title", &state.content.document_title);
    insert_opt(
        &mut m,
        "focused_element_role",
        &state.content.focused_element_role,
    );
    insert_opt(
        &mut m,
        "focused_element_title",
        &state.content.focused_element_title,
    );

    // Honour character budget for potentially large excerpt.
    let field_budget = opts.char_budget / 4;
    if let Some(ref excerpt) = state.content.visible_text_excerpt {
        let truncated = if excerpt.chars().count() > field_budget {
            excerpt.chars().take(field_budget).collect::<String>()
        } else {
            excerpt.clone()
        };
        m.insert("visible_text_excerpt".to_owned(), json!(truncated));
    }

    if !state.content.form_fields.is_empty() {
        let fields: Value = state
            .content
            .form_fields
            .iter()
            .map(|(k, v)| json!({"label": k, "value": v}))
            .collect::<Vec<_>>()
            .into();
        m.insert("form_fields".to_owned(), fields);
    }

    Value::Object(m)
}

// ---------------------------------------------------------------------------
// Primitive helpers
// ---------------------------------------------------------------------------

fn push_section(buf: &mut String, name: &str) {
    buf.push('[');
    buf.push_str(name);
    buf.push_str("]\n");
}

fn push_kv(buf: &mut String, key: &str, value: &str) {
    buf.push_str(key);
    buf.push_str(": ");
    buf.push_str(value);
    buf.push('\n');
}

fn push_opt_kv(buf: &mut String, key: &str, value: &Option<String>) {
    if let Some(v) = value {
        push_kv(buf, key, v);
    }
}

fn push_line(buf: &mut String, line: &str) {
    buf.push_str(line);
    buf.push('\n');
}

fn insert_opt(map: &mut serde_json::Map<String, Value>, key: &str, value: &Option<String>) {
    if let Some(v) = value {
        map.insert(key.to_owned(), json!(v));
    }
}

fn truncate_to_budget(mut s: String, budget: usize) -> String {
    if s.len() <= budget {
        return s;
    }
    // Truncate at char boundary to avoid broken UTF-8.
    let end = s
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i < budget)
        .last()
        .unwrap_or(0);
    s.truncate(end);
    s.push_str("\n...[truncated]");
    s
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copilot_state::{CopilotState, StateChange};

    fn state_with_app(name: &str) -> CopilotState {
        let mut s = CopilotState::empty();
        s.app.name = Some(name.to_owned());
        s
    }

    // -- format_for_llm: basic structure ------------------------------------

    #[test]
    fn format_for_llm_empty_state_is_non_empty_string() {
        // GIVEN
        let state = CopilotState::empty();

        // WHEN
        let text = format_for_llm(&state, &FormatOptions::default());

        // THEN: always produces at least the App section header
        assert!(!text.is_empty());
    }

    #[test]
    fn format_for_llm_includes_app_name() {
        // GIVEN
        let state = state_with_app("Safari");

        // WHEN
        let text = format_for_llm(&state, &FormatOptions::default());

        // THEN
        assert!(text.contains("Safari"), "got: {text}");
    }

    #[test]
    fn format_for_llm_omits_none_fields() {
        // GIVEN: state with only app name set
        let state = state_with_app("Finder");

        // WHEN
        let text = format_for_llm(&state, &FormatOptions::default());

        // THEN: fields that are None do not appear as empty "key: " lines
        assert!(!text.contains("focused_window:"), "got: {text}");
    }

    #[test]
    fn format_for_llm_includes_prefix_when_set() {
        // GIVEN
        let state = CopilotState::empty();
        let opts = FormatOptions {
            prefix: Some("## App State".to_owned()),
            ..FormatOptions::default()
        };

        // WHEN
        let text = format_for_llm(&state, &opts);

        // THEN
        assert!(text.starts_with("## App State"), "got: {text}");
    }

    #[test]
    fn format_for_llm_respects_char_budget() {
        // GIVEN: state with lots of content
        let mut state = CopilotState::empty();
        state.content.visible_text_excerpt = Some("x".repeat(10_000));

        let opts = FormatOptions {
            char_budget: 500,
            ..FormatOptions::default()
        };

        // WHEN
        let text = format_for_llm(&state, &opts);

        // THEN: output is under budget (plus truncation marker)
        assert!(text.len() < 600, "len={}", text.len());
    }

    #[test]
    fn format_for_llm_includes_timestamp_when_requested() {
        // GIVEN
        let mut state = CopilotState::empty();
        state.timestamp = 1_700_000_000;
        let opts = FormatOptions {
            include_timestamp: true,
            ..FormatOptions::default()
        };

        // WHEN
        let text = format_for_llm(&state, &opts);

        // THEN
        assert!(text.contains("1700000000"), "got: {text}");
    }

    // -- format_for_llm: sections appear only when populated ---------------

    #[test]
    fn format_for_llm_shows_selection_section_when_populated() {
        // GIVEN
        let mut state = CopilotState::empty();
        state.selection.selected_text = Some("hello".to_owned());

        // WHEN
        let text = format_for_llm(&state, &FormatOptions::default());

        // THEN
        assert!(text.contains("[Selection]"), "got: {text}");
    }

    #[test]
    fn format_for_llm_omits_selection_section_when_empty() {
        // GIVEN: no selection
        let state = CopilotState::empty();

        // WHEN
        let text = format_for_llm(&state, &FormatOptions::default());

        // THEN
        assert!(!text.contains("[Selection]"), "got: {text}");
    }

    #[test]
    fn format_for_llm_shows_navigation_breadcrumb() {
        // GIVEN
        let mut state = CopilotState::empty();
        state.navigation.breadcrumb = vec!["Home".to_owned(), "Settings".to_owned()];

        // WHEN
        let text = format_for_llm(&state, &FormatOptions::default());

        // THEN: breadcrumb rendered with separator
        assert!(text.contains("Home > Settings"), "got: {text}");
    }

    // -- format_as_json: structure ------------------------------------------

    #[test]
    fn format_as_json_returns_object() {
        // GIVEN / WHEN
        let json = format_as_json(&CopilotState::empty(), &FormatOptions::default());

        // THEN
        assert!(json.is_object());
    }

    #[test]
    fn format_as_json_has_required_top_level_keys() {
        // GIVEN / WHEN
        let json = format_as_json(&CopilotState::empty(), &FormatOptions::default());

        // THEN
        assert!(json["app"].is_object());
        assert!(json["selection"].is_object());
        assert!(json["navigation"].is_object());
        assert!(json["content"].is_object());
    }

    #[test]
    fn format_as_json_includes_app_name() {
        // GIVEN
        let state = state_with_app("Xcode");

        // WHEN
        let json = format_as_json(&state, &FormatOptions::default());

        // THEN
        assert_eq!(json["app"]["name"], "Xcode");
    }

    #[test]
    fn format_as_json_omits_none_fields() {
        // GIVEN: empty state
        let json = format_as_json(&CopilotState::empty(), &FormatOptions::default());

        // THEN: none of the optional string keys are present
        assert!(json["app"]["name"].is_null(), "got: {:?}", json["app"]);
    }

    #[test]
    fn format_as_json_includes_timestamp_when_requested() {
        // GIVEN
        let mut state = CopilotState::empty();
        state.timestamp = 1_234_567_890;
        let opts = FormatOptions {
            include_timestamp: true,
            ..FormatOptions::default()
        };

        // WHEN
        let json = format_as_json(&state, &opts);

        // THEN
        assert_eq!(json["timestamp"], 1_234_567_890_u64);
    }

    #[test]
    fn format_as_json_navigation_depth_always_present() {
        // GIVEN
        let mut state = CopilotState::empty();
        state.navigation.depth = 7;

        // WHEN
        let json = format_as_json(&state, &FormatOptions::default());

        // THEN
        assert_eq!(json["navigation"]["depth"], 7);
    }

    // -- format_changes_for_llm ---------------------------------------------

    #[test]
    fn format_changes_for_llm_empty_returns_no_changes_message() {
        // GIVEN / WHEN
        let text = format_changes_for_llm(&[], &FormatOptions::default());

        // THEN
        assert!(text.contains("No state changes"), "got: {text}");
    }

    #[test]
    fn format_changes_for_llm_lists_all_changes() {
        // GIVEN
        let changes = vec![
            StateChange {
                field: "app.name".to_owned(),
                old_value: "null".to_owned(),
                new_value: "\"Safari\"".to_owned(),
            },
            StateChange {
                field: "navigation.depth".to_owned(),
                old_value: "0".to_owned(),
                new_value: "3".to_owned(),
            },
        ];

        // WHEN
        let text = format_changes_for_llm(&changes, &FormatOptions::default());

        // THEN
        assert!(text.contains("app.name"), "got: {text}");
        assert!(text.contains("navigation.depth"), "got: {text}");
    }

    // -- FormatOptions --------------------------------------------------

    #[test]
    fn format_options_with_token_budget_sets_char_budget() {
        // GIVEN / WHEN
        let opts = FormatOptions::with_token_budget(1024);

        // THEN: 1024 tokens × 4 chars/token
        assert_eq!(opts.char_budget, 4096);
    }

    // -- truncate_to_budget --------------------------------------------------

    #[test]
    fn truncate_to_budget_short_string_unchanged() {
        // GIVEN / WHEN / THEN
        let s = "hello world".to_owned();
        assert_eq!(truncate_to_budget(s, 100), "hello world");
    }

    #[test]
    fn truncate_to_budget_long_string_appends_marker() {
        // GIVEN
        let s = "a".repeat(200);

        // WHEN
        let result = truncate_to_budget(s, 50);

        // THEN
        assert!(result.ends_with("[truncated]"), "got: {result}");
        assert!(result.len() <= 70, "len={}", result.len()); // 50 + marker
    }
}
