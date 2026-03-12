//! Integration tests for `copilot_state` and `copilot_format` modules.
//!
//! All tests are pure logic tests that do not require a running macOS app or
//! accessibility permissions. Live AX tests are guarded by a permissions check
//! and skipped gracefully when permissions are absent.

use axterminator::copilot_format::{
    format_as_json, format_changes_for_llm, format_for_llm, FormatOptions,
};
use axterminator::copilot_state::{
    diff_states, AppContext, ContentContext, CopilotState, NavigationContext, SelectionContext,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn full_state() -> CopilotState {
    CopilotState {
        app: AppContext {
            name: Some("Safari".to_owned()),
            focused_window: Some("Main Window".to_owned()),
            active_tab: Some("Tab 1".to_owned()),
            active_document: Some("index.html".to_owned()),
        },
        selection: SelectionContext {
            selected_text: Some("Hello world".to_owned()),
            selected_list_row: Some(2),
            selected_table_cell: None,
            selected_items: vec!["Item A".to_owned(), "Item B".to_owned()],
        },
        navigation: NavigationContext {
            breadcrumb: vec!["Home".to_owned(), "Projects".to_owned()],
            sidebar_selection: Some("Inbox".to_owned()),
            tab_bar_selection: Some("Editor".to_owned()),
            depth: 3,
        },
        content: ContentContext {
            document_title: Some("My Project".to_owned()),
            visible_text_excerpt: Some("Lorem ipsum dolor".to_owned()),
            form_fields: vec![
                ("Username".to_owned(), "alice".to_owned()),
                ("Email".to_owned(), "alice@example.com".to_owned()),
            ],
            focused_element_role: Some("AXTextField".to_owned()),
            focused_element_title: Some("Search".to_owned()),
        },
        timestamp: 1_700_000_000,
    }
}

// ---------------------------------------------------------------------------
// CopilotState construction
// ---------------------------------------------------------------------------

#[test]
fn empty_state_all_fields_default() {
    // GIVEN / WHEN
    let state = CopilotState::empty();

    // THEN
    assert!(state.app.name.is_none());
    assert!(state.selection.selected_items.is_empty());
    assert!(state.navigation.breadcrumb.is_empty());
    assert!(state.content.form_fields.is_empty());
}

#[test]
fn full_state_carries_all_fields() {
    // GIVEN / WHEN
    let s = full_state();

    // THEN: spot-check every context
    assert_eq!(s.app.name.as_deref(), Some("Safari"));
    assert_eq!(s.selection.selected_list_row, Some(2));
    assert_eq!(s.navigation.depth, 3);
    assert_eq!(s.content.form_fields.len(), 2);
}

// ---------------------------------------------------------------------------
// diff_states — no change
// ---------------------------------------------------------------------------

#[test]
fn diff_identical_empty_states_empty_vec() {
    // GIVEN
    let s = CopilotState::empty();

    // WHEN / THEN
    assert!(diff_states(&s, &s).is_empty());
}

#[test]
fn diff_identical_full_states_empty_vec() {
    // GIVEN
    let s = full_state();

    // WHEN / THEN
    assert!(diff_states(&s, &s).is_empty());
}

// ---------------------------------------------------------------------------
// diff_states — AppContext
// ---------------------------------------------------------------------------

#[test]
fn diff_app_name_none_to_some() {
    // GIVEN
    let old = CopilotState::empty();
    let mut new = CopilotState::empty();
    new.app.name = Some("Finder".to_owned());

    // WHEN
    let changes = diff_states(&old, &new);

    // THEN
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].field, "app.name");
    assert!(changes[0].new_value.contains("Finder"));
}

#[test]
fn diff_app_focused_window_changes() {
    // GIVEN
    let mut old = full_state();
    let mut new = full_state();
    old.app.focused_window = Some("Win A".to_owned());
    new.app.focused_window = Some("Win B".to_owned());

    // WHEN
    let changes = diff_states(&old, &new);

    // THEN
    assert!(changes.iter().any(|c| c.field == "app.focused_window"));
}

// ---------------------------------------------------------------------------
// diff_states — SelectionContext
// ---------------------------------------------------------------------------

#[test]
fn diff_selected_text_changes() {
    // GIVEN
    let mut old = CopilotState::empty();
    let mut new = CopilotState::empty();
    old.selection.selected_text = Some("foo".to_owned());
    new.selection.selected_text = Some("foo bar".to_owned());

    // WHEN / THEN
    let changes = diff_states(&old, &new);
    assert!(changes.iter().any(|c| c.field == "selection.selected_text"));
}

#[test]
fn diff_selected_list_row_changes() {
    // GIVEN
    let mut old = CopilotState::empty();
    let mut new = CopilotState::empty();
    old.selection.selected_list_row = Some(0);
    new.selection.selected_list_row = Some(9);

    // WHEN / THEN
    let changes = diff_states(&old, &new);
    assert!(changes
        .iter()
        .any(|c| c.field == "selection.selected_list_row"));
}

#[test]
fn diff_selected_items_vec_changes() {
    // GIVEN
    let old = CopilotState::empty();
    let mut new = CopilotState::empty();
    new.selection.selected_items = vec!["X".to_owned()];

    // WHEN / THEN
    let changes = diff_states(&old, &new);
    assert!(changes.iter().any(|c| c.field == "selection.selected_items"));
}

// ---------------------------------------------------------------------------
// diff_states — NavigationContext
// ---------------------------------------------------------------------------

#[test]
fn diff_breadcrumb_changes() {
    // GIVEN
    let old = CopilotState::empty();
    let mut new = CopilotState::empty();
    new.navigation.breadcrumb = vec!["Root".to_owned()];

    // WHEN / THEN
    let changes = diff_states(&old, &new);
    assert!(changes.iter().any(|c| c.field == "navigation.breadcrumb"));
}

#[test]
fn diff_navigation_depth_changes() {
    // GIVEN
    let old = CopilotState::empty();
    let mut new = CopilotState::empty();
    new.navigation.depth = 5;

    // WHEN / THEN
    let changes = diff_states(&old, &new);
    assert!(changes.iter().any(|c| c.field == "navigation.depth"));
}

// ---------------------------------------------------------------------------
// diff_states — ContentContext
// ---------------------------------------------------------------------------

#[test]
fn diff_document_title_changes() {
    // GIVEN
    let old = CopilotState::empty();
    let mut new = CopilotState::empty();
    new.content.document_title = Some("Report.pdf".to_owned());

    // WHEN / THEN
    let changes = diff_states(&old, &new);
    assert!(changes.iter().any(|c| c.field == "content.document_title"));
}

#[test]
fn diff_form_fields_changes() {
    // GIVEN
    let old = CopilotState::empty();
    let mut new = CopilotState::empty();
    new.content.form_fields = vec![("Name".to_owned(), "Bob".to_owned())];

    // WHEN / THEN
    let changes = diff_states(&old, &new);
    assert!(changes.iter().any(|c| c.field == "content.form_fields"));
}

#[test]
fn diff_multiple_changes_all_reported() {
    // GIVEN
    let old = CopilotState::empty();
    let mut new = CopilotState::empty();
    new.app.name = Some("Xcode".to_owned());
    new.navigation.depth = 2;
    new.content.document_title = Some("main.rs".to_owned());

    // WHEN
    let changes = diff_states(&old, &new);

    // THEN: exactly 3 changes
    assert_eq!(changes.len(), 3);
}

// ---------------------------------------------------------------------------
// format_for_llm
// ---------------------------------------------------------------------------

#[test]
fn format_for_llm_full_state_contains_all_sections() {
    // GIVEN
    let state = full_state();
    let opts = FormatOptions::default();

    // WHEN
    let text = format_for_llm(&state, &opts);

    // THEN
    assert!(text.contains("[App]"), "missing [App]: {text}");
    assert!(text.contains("[Selection]"), "missing [Selection]: {text}");
    assert!(text.contains("[Navigation]"), "missing [Navigation]: {text}");
    assert!(text.contains("[Content]"), "missing [Content]: {text}");
}

#[test]
fn format_for_llm_breadcrumb_uses_arrow_separator() {
    // GIVEN
    let mut state = CopilotState::empty();
    state.navigation.breadcrumb = vec!["A".to_owned(), "B".to_owned(), "C".to_owned()];

    // WHEN
    let text = format_for_llm(&state, &FormatOptions::default());

    // THEN
    assert!(text.contains("A > B > C"), "got: {text}");
}

#[test]
fn format_for_llm_selected_items_comma_joined() {
    // GIVEN
    let mut state = CopilotState::empty();
    state.selection.selected_items = vec!["One".to_owned(), "Two".to_owned()];

    // WHEN
    let text = format_for_llm(&state, &FormatOptions::default());

    // THEN
    assert!(text.contains("One, Two"), "got: {text}");
}

#[test]
fn format_for_llm_token_budget_via_with_token_budget() {
    // GIVEN: budget of 32 tokens = 128 chars
    let state = full_state();
    let opts = FormatOptions::with_token_budget(32);

    // WHEN
    let text = format_for_llm(&state, &opts);

    // THEN: output truncated with marker
    assert!(text.contains("[truncated]"), "got: {text}");
}

// ---------------------------------------------------------------------------
// format_as_json
// ---------------------------------------------------------------------------

#[test]
fn format_as_json_full_state_all_keys_present() {
    // GIVEN
    let state = full_state();
    let opts = FormatOptions::default();

    // WHEN
    let json = format_as_json(&state, &opts);

    // THEN
    assert_eq!(json["app"]["name"], "Safari");
    assert_eq!(json["navigation"]["depth"], 3);
    assert!(json["selection"]["selected_items"].is_array());
    assert!(json["content"]["form_fields"].is_array());
}

#[test]
fn format_as_json_form_fields_as_object_array() {
    // GIVEN
    let mut state = CopilotState::empty();
    state.content.form_fields = vec![("User".to_owned(), "carol".to_owned())];

    // WHEN
    let json = format_as_json(&state, &FormatOptions::default());

    // THEN: form_fields is an array of {label, value} objects
    let fields = &json["content"]["form_fields"];
    assert!(fields.is_array());
    assert_eq!(fields[0]["label"], "User");
    assert_eq!(fields[0]["value"], "carol");
}

#[test]
fn format_as_json_visible_text_truncated_to_quarter_budget() {
    // GIVEN: 100-token budget → 400 char budget → excerpt capped at 100 chars
    let mut state = CopilotState::empty();
    state.content.visible_text_excerpt = Some("z".repeat(1000));

    let opts = FormatOptions::with_token_budget(100); // 400 char budget

    // WHEN
    let json = format_as_json(&state, &opts);

    // THEN: excerpt ≤ 100 chars (400 / 4)
    let excerpt = json["content"]["visible_text_excerpt"]
        .as_str()
        .expect("should be string");
    assert!(excerpt.len() <= 100, "len={}", excerpt.len());
}

// ---------------------------------------------------------------------------
// format_changes_for_llm
// ---------------------------------------------------------------------------

#[test]
fn format_changes_empty_message() {
    // GIVEN / WHEN
    let text = format_changes_for_llm(&[], &FormatOptions::default());

    // THEN
    assert!(text.contains("No state changes"));
}

#[test]
fn format_changes_lists_field_names() {
    // GIVEN
    let old = CopilotState::empty();
    let mut new = CopilotState::empty();
    new.app.name = Some("Notes".to_owned());
    new.navigation.depth = 1;
    let changes = diff_states(&old, &new);

    // WHEN
    let text = format_changes_for_llm(&changes, &FormatOptions::default());

    // THEN
    assert!(text.contains("app.name"), "got: {text}");
    assert!(text.contains("navigation.depth"), "got: {text}");
}

// ---------------------------------------------------------------------------
// Null-ref safety (no live accessibility required)
// ---------------------------------------------------------------------------

#[test]
fn read_copilot_state_null_ref_does_not_crash() {
    // GIVEN: null element pointer
    let null_ref: axterminator::AXUIElementRef = std::ptr::null();

    // WHEN: state extraction — must return gracefully, never panic
    let state = axterminator::copilot_state::read_copilot_state(null_ref);

    // THEN: result is an empty state
    assert!(state.app.name.is_none());
}

// ---------------------------------------------------------------------------
// Live accessibility tests (skipped when permissions absent)
// ---------------------------------------------------------------------------

#[test]
fn live_read_copilot_state_finder() {
    if !axterminator::check_accessibility_enabled() {
        eprintln!("Skipping: accessibility not enabled");
        return;
    }

    // Find Finder PID
    let output = std::process::Command::new("pgrep")
        .args(["-x", "Finder"])
        .output()
        .expect("pgrep failed");

    let pid_str = String::from_utf8_lossy(&output.stdout);
    let pid: i32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            eprintln!("Skipping: Finder not running");
            return;
        }
    };

    let app_ref = axterminator::create_application_element(pid)
        .expect("create_application_element failed");

    // WHEN: extract state — should never panic
    let state = axterminator::copilot_state::read_copilot_state(app_ref);

    // THEN: timestamp is set and format produces non-empty output
    assert!(state.timestamp > 0);
    let text = format_for_llm(&state, &FormatOptions::default());
    assert!(!text.is_empty());
}
