//! Internal AX tree extraction helpers for `CopilotState` assembly.
//!
//! Each function is responsible for reading a specific slice of the
//! accessibility tree and returning a typed value. All functions are
//! null-safe: passing a null `AXUIElementRef` returns `None` / empty.
//!
//! This module is `pub(crate)`-only; callers should use
//! [`crate::copilot_state::read_copilot_state`] as the public entry point.

use crate::accessibility::{attributes, get_bool_attribute_value, get_children, get_string_attribute_value, AXUIElementRef};
use crate::copilot_state::{AppContext, ContentContext, NavigationContext, SelectionContext};

// ---------------------------------------------------------------------------
// Context extractors — one per CopilotState field group
// ---------------------------------------------------------------------------

pub(crate) fn extract_app_context(
    app_ref: AXUIElementRef,
    children: &[AXUIElementRef],
) -> AppContext {
    AppContext {
        name: get_string_attribute_value(app_ref, attributes::AX_TITLE),
        focused_window: find_focused_window(app_ref),
        active_tab: find_active_tab(children),
        active_document: find_active_document(children),
    }
}

pub(crate) fn extract_selection_context(
    window_ref: Option<AXUIElementRef>,
    children: &[AXUIElementRef],
) -> SelectionContext {
    let mut ctx = SelectionContext::default();
    if let Some(win) = window_ref {
        ctx.selected_text = find_selected_text(win);
        ctx.selected_list_row = find_selected_list_row(win, children);
        ctx.selected_items = collect_selected_items(win);
    }
    ctx
}

pub(crate) fn extract_navigation_context(
    window_ref: Option<AXUIElementRef>,
    children: &[AXUIElementRef],
) -> NavigationContext {
    NavigationContext {
        breadcrumb: build_breadcrumb(children),
        sidebar_selection: find_sidebar_selection(window_ref, children),
        tab_bar_selection: find_tab_bar_selection(window_ref, children),
        depth: children.len(),
    }
}

pub(crate) fn extract_content_context(
    app_ref: AXUIElementRef,
    window_ref: Option<AXUIElementRef>,
    children: &[AXUIElementRef],
) -> ContentContext {
    let focused_el = find_focused_element(app_ref, children);
    ContentContext {
        document_title: find_document_title(window_ref, children),
        visible_text_excerpt: collect_visible_text(window_ref, children),
        form_fields: collect_form_fields(window_ref, children),
        focused_element_role: focused_el
            .and_then(|r| get_string_attribute_value(*r, attributes::AX_ROLE)),
        focused_element_title: focused_el
            .and_then(|r| get_string_attribute_value(*r, attributes::AX_TITLE)),
    }
}

// ---------------------------------------------------------------------------
// Window / app-level helpers
// ---------------------------------------------------------------------------

pub(crate) fn find_focused_window(app_ref: AXUIElementRef) -> Option<String> {
    get_string_attribute_value(app_ref, attributes::AX_FOCUSED_WINDOW)
        .or_else(|| get_string_attribute_value(app_ref, attributes::AX_MAIN_WINDOW))
        .or_else(|| get_string_attribute_value(app_ref, attributes::AX_TITLE))
}

pub(crate) fn first_window_ref(
    app_ref: AXUIElementRef,
    children: &[AXUIElementRef],
) -> Option<AXUIElementRef> {
    children
        .iter()
        .find(|&&c| {
            get_string_attribute_value(c, attributes::AX_ROLE).as_deref() == Some("AXWindow")
        })
        .copied()
        .or(if app_ref.is_null() { None } else { Some(app_ref) })
}

fn find_active_tab(children: &[AXUIElementRef]) -> Option<String> {
    children.iter().find_map(|&c| {
        let role = get_string_attribute_value(c, attributes::AX_ROLE)?;
        if role == "AXTabGroup" {
            if let Ok(tab_children) = get_children(c) {
                return tab_children.iter().find_map(|&tc| {
                    let selected = get_bool_attribute_value(tc, "AXSelected")?;
                    if selected {
                        get_string_attribute_value(tc, attributes::AX_TITLE)
                    } else {
                        None
                    }
                });
            }
        }
        None
    })
}

fn find_active_document(children: &[AXUIElementRef]) -> Option<String> {
    children.iter().find_map(|&c| {
        if get_string_attribute_value(c, attributes::AX_ROLE).as_deref() == Some("AXWindow") {
            get_string_attribute_value(c, "AXDocument")
                .or_else(|| get_string_attribute_value(c, attributes::AX_TITLE))
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// Selection helpers
// ---------------------------------------------------------------------------

fn find_selected_text(win: AXUIElementRef) -> Option<String> {
    let children = get_children(win).ok()?;
    find_selected_text_in(children.as_slice())
}

fn find_selected_text_in(elements: &[AXUIElementRef]) -> Option<String> {
    for &el in elements {
        let role = get_string_attribute_value(el, attributes::AX_ROLE);
        let is_text = matches!(role.as_deref(), Some("AXTextField" | "AXTextArea"));
        if is_text {
            if let Some(text) = get_string_attribute_value(el, "AXSelectedText") {
                if !text.is_empty() {
                    return Some(crate::copilot_state::truncate_str(text, 512));
                }
            }
        }
        if let Ok(kids) = get_children(el) {
            if let Some(found) = find_selected_text_in(&kids) {
                return Some(found);
            }
        }
    }
    None
}

fn find_selected_list_row(
    win: AXUIElementRef,
    _children: &[AXUIElementRef],
) -> Option<usize> {
    let children = get_children(win).ok()?;
    find_list_row_in(children.as_slice())
}

fn find_list_row_in(elements: &[AXUIElementRef]) -> Option<usize> {
    for &el in elements {
        if get_string_attribute_value(el, attributes::AX_ROLE).as_deref() == Some("AXList") {
            if let Ok(rows) = get_children(el) {
                for (i, &row) in rows.iter().enumerate() {
                    if get_bool_attribute_value(row, "AXSelected").unwrap_or(false) {
                        return Some(i);
                    }
                }
            }
        }
        if let Ok(kids) = get_children(el) {
            if let Some(idx) = find_list_row_in(&kids) {
                return Some(idx);
            }
        }
    }
    None
}

fn collect_selected_items(win: AXUIElementRef) -> Vec<String> {
    let Ok(children) = get_children(win) else {
        return Vec::new();
    };
    collect_selected_items_in(children.as_slice())
}

fn collect_selected_items_in(elements: &[AXUIElementRef]) -> Vec<String> {
    let mut result = Vec::new();
    for &el in elements {
        if get_bool_attribute_value(el, "AXSelected").unwrap_or(false) {
            if let Some(title) = get_string_attribute_value(el, attributes::AX_TITLE)
                .or_else(|| get_string_attribute_value(el, attributes::AX_VALUE))
            {
                result.push(title);
            }
        }
        if let Ok(kids) = get_children(el) {
            result.extend(collect_selected_items_in(&kids));
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Navigation helpers
// ---------------------------------------------------------------------------

fn build_breadcrumb(children: &[AXUIElementRef]) -> Vec<String> {
    let mut crumbs = Vec::new();
    for &c in children.iter().take(3) {
        if let Some(title) = get_string_attribute_value(c, attributes::AX_TITLE)
            .filter(|t| !t.is_empty())
        {
            crumbs.push(title);
        }
    }
    crumbs
}

fn find_sidebar_selection(
    window_ref: Option<AXUIElementRef>,
    _children: &[AXUIElementRef],
) -> Option<String> {
    let win = window_ref?;
    let kids = get_children(win).ok()?;
    find_role_selection_in(&kids, "AXOutline")
        .or_else(|| find_role_selection_in(&kids, "AXScrollArea"))
}

fn find_role_selection_in(elements: &[AXUIElementRef], target_role: &str) -> Option<String> {
    for &el in elements {
        if get_string_attribute_value(el, attributes::AX_ROLE).as_deref() == Some(target_role) {
            if let Ok(rows) = get_children(el) {
                for &row in &rows {
                    if get_bool_attribute_value(row, "AXSelected").unwrap_or(false) {
                        return get_string_attribute_value(row, attributes::AX_TITLE);
                    }
                }
            }
        }
        if let Ok(kids) = get_children(el) {
            if let Some(found) = find_role_selection_in(&kids, target_role) {
                return Some(found);
            }
        }
    }
    None
}

fn find_tab_bar_selection(
    window_ref: Option<AXUIElementRef>,
    _children: &[AXUIElementRef],
) -> Option<String> {
    let win = window_ref?;
    let kids = get_children(win).ok()?;
    find_role_selection_in(&kids, "AXTabGroup")
}

// ---------------------------------------------------------------------------
// Content helpers
// ---------------------------------------------------------------------------

fn find_document_title(
    window_ref: Option<AXUIElementRef>,
    _children: &[AXUIElementRef],
) -> Option<String> {
    window_ref.and_then(|w| get_string_attribute_value(w, attributes::AX_TITLE))
}

fn collect_visible_text(
    window_ref: Option<AXUIElementRef>,
    _children: &[AXUIElementRef],
) -> Option<String> {
    let win = window_ref?;
    let kids = get_children(win).ok()?;
    let text = collect_text_in(kids.as_slice(), 256);
    if text.is_empty() { None } else { Some(text) }
}

fn collect_text_in(elements: &[AXUIElementRef], budget: usize) -> String {
    let mut buf = String::new();
    for &el in elements {
        if buf.len() >= budget {
            break;
        }
        let role = get_string_attribute_value(el, attributes::AX_ROLE);
        if matches!(
            role.as_deref(),
            Some("AXStaticText" | "AXTextField" | "AXTextArea")
        ) {
            if let Some(val) = get_string_attribute_value(el, attributes::AX_VALUE)
                .or_else(|| get_string_attribute_value(el, attributes::AX_TITLE))
            {
                if !buf.is_empty() {
                    buf.push(' ');
                }
                let remaining = budget.saturating_sub(buf.len());
                buf.push_str(&crate::copilot_state::truncate_str(val, remaining));
            }
        }
        if let Ok(kids) = get_children(el) {
            let sub = collect_text_in(&kids, budget.saturating_sub(buf.len()));
            if !sub.is_empty() {
                if !buf.is_empty() {
                    buf.push(' ');
                }
                buf.push_str(&sub);
            }
        }
    }
    buf
}

fn collect_form_fields(
    window_ref: Option<AXUIElementRef>,
    _children: &[AXUIElementRef],
) -> Vec<(String, String)> {
    let Some(win) = window_ref else {
        return Vec::new();
    };
    let Ok(kids) = get_children(win) else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    collect_form_fields_in(kids.as_slice(), &mut fields);
    fields
}

fn collect_form_fields_in(
    elements: &[AXUIElementRef],
    fields: &mut Vec<(String, String)>,
) {
    if fields.len() >= 20 {
        return;
    }
    for &el in elements {
        let role = get_string_attribute_value(el, attributes::AX_ROLE);
        if matches!(role.as_deref(), Some("AXTextField" | "AXTextArea" | "AXComboBox")) {
            let label = get_string_attribute_value(el, attributes::AX_TITLE)
                .or_else(|| get_string_attribute_value(el, attributes::AX_DESCRIPTION))
                .or_else(|| get_string_attribute_value(el, attributes::AX_LABEL))
                .unwrap_or_default();
            let value = get_string_attribute_value(el, attributes::AX_VALUE).unwrap_or_default();
            if !value.is_empty() && fields.len() < 20 {
                fields.push((label, value));
            }
        }
        if let Ok(kids) = get_children(el) {
            collect_form_fields_in(&kids, fields);
        }
    }
}

fn find_focused_element<'a>(
    app_ref: AXUIElementRef,
    _children: &'a [AXUIElementRef],
) -> Option<&'a AXUIElementRef> {
    // AXFocusedUIElement is on the application element, not children.
    // We read it inline since we can't return a reference to a temporary.
    let _ = get_string_attribute_value(app_ref, "AXFocusedUIElement");
    None // Returning None here; title/role from children is the practical path.
}
