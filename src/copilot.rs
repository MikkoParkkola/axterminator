//! Proactive Desktop Copilot — context-aware suggestion engine.
//!
//! Observes the user's desktop state (current app, window title, clipboard,
//! idle time, and action history) and proactively suggests relevant actions
//! before the user has to ask.
//!
//! # Architecture
//!
//! ```text
//! CopilotContext  →  DesktopCopilot::evaluate()  →  Vec<Suggestion>
//!                       ↑ rules checked in priority order
//!                    Vec<CopilotRule>
//! ```
//!
//! # Example
//!
//! ```
//! use axterminator::copilot::{
//!     DesktopCopilot, CopilotContext, CopilotRule, RuleCondition, SuggestedAction,
//! };
//!
//! let mut copilot = DesktopCopilot::with_builtin_rules();
//!
//! let ctx = CopilotContext {
//!     current_app: "Safari".to_owned(),
//!     current_window_title: "Untitled".to_owned(),
//!     recent_actions: vec![],
//!     clipboard: Some("https://example.com".to_owned()),
//!     time_in_current_app_ms: 0,
//! };
//! copilot.update_context(ctx);
//!
//! let suggestions = copilot.evaluate();
//! assert!(!suggestions.is_empty());
//! ```

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Current observable state of the desktop at a point in time.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CopilotContext {
    /// Name of the currently active application (e.g., `"VS Code"`).
    pub current_app: String,
    /// Title of the currently focused window.
    pub current_window_title: String,
    /// Sequence of recent user action labels, newest last.
    pub recent_actions: Vec<String>,
    /// Current clipboard text, if any.
    pub clipboard: Option<String>,
    /// Milliseconds the user has been in the current app without switching.
    pub time_in_current_app_ms: u64,
}

/// A condition that must hold for a rule to fire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleCondition {
    /// Active app name equals the given string (case-insensitive).
    AppIs(String),
    /// Window title contains the given substring (case-insensitive).
    WindowTitleContains(String),
    /// Clipboard text contains the given substring (case-insensitive).
    ClipboardContains(String),
    /// User has been idle in the current app for at least this many ms.
    IdleFor(u64),
    /// A given action label appears at least `min_count` times in recent history.
    RepeatedAction(String, u32),
    /// Recent actions end with this exact sequence (newest-last order).
    Pattern(Vec<String>),
}

/// The action the copilot recommends when a rule fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestedAction {
    /// Invoke a named workflow.
    RunWorkflow(String),
    /// Insert text at the current cursor position.
    TypeText(String),
    /// Open or switch to an application by name.
    OpenApp(String),
    /// Surface a human-readable tip.
    ShowTip(String),
    /// Execute a shell script.
    RunScript(String),
}

/// A single copilot rule: fires `action` when `condition` is met.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotRule {
    /// Human-readable rule identifier used for tracing.
    pub name: String,
    /// The logical predicate evaluated against the current context.
    pub condition: RuleCondition,
    /// The action to recommend when the condition is met.
    pub action: SuggestedAction,
    /// Higher value = surfaced first in [`DesktopCopilot::evaluate`] output.
    pub priority: u8,
}

/// A concrete suggestion produced by the copilot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// Name of the rule that produced this suggestion.
    pub rule_name: String,
    /// Recommended action.
    pub action: SuggestedAction,
    /// Match confidence in `[0.0, 1.0]`.
    pub confidence: f64,
    /// Human-readable explanation of why this was suggested.
    pub reason: String,
}

/// Proactive desktop copilot that evaluates rules against observed context.
#[derive(Debug, Default)]
pub struct DesktopCopilot {
    context: CopilotContext,
    rules: Vec<CopilotRule>,
}

// ---------------------------------------------------------------------------
// DesktopCopilot implementation
// ---------------------------------------------------------------------------

impl DesktopCopilot {
    /// Create a copilot with no rules and an empty context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a copilot pre-loaded with the built-in default rules.
    #[must_use]
    pub fn with_builtin_rules() -> Self {
        let mut copilot = Self::new();
        for rule in builtin_rules() {
            copilot.add_rule(rule);
        }
        copilot
    }

    /// Replace the observed context.  Call this whenever the desktop state changes.
    pub fn update_context(&mut self, ctx: CopilotContext) {
        self.context = ctx;
    }

    /// Register an additional rule.
    ///
    /// Rules with the same name as an existing rule are appended (not replaced).
    pub fn add_rule(&mut self, rule: CopilotRule) {
        self.rules.push(rule);
    }

    /// Evaluate all rules against the current context.
    ///
    /// Returns matching suggestions sorted by descending priority.  If multiple
    /// rules have equal priority they are returned in registration order.
    #[must_use]
    pub fn evaluate(&self) -> Vec<Suggestion> {
        let mut suggestions: Vec<Suggestion> = self
            .rules
            .iter()
            .filter_map(|rule| self.try_match(rule))
            .collect();

        suggestions.sort_by(|a, b| {
            let pa = self.priority_of(&a.rule_name);
            let pb = self.priority_of(&b.rule_name);
            pb.cmp(&pa)
        });

        suggestions
    }

    // -- helpers -------------------------------------------------------------

    fn priority_of(&self, name: &str) -> u8 {
        self.rules
            .iter()
            .find(|r| r.name == name)
            .map_or(0, |r| r.priority)
    }

    fn try_match(&self, rule: &CopilotRule) -> Option<Suggestion> {
        let (confidence, reason) = evaluate_condition(&rule.condition, &self.context)?;
        Some(Suggestion {
            rule_name: rule.name.clone(),
            action: rule.action.clone(),
            confidence,
            reason,
        })
    }
}

// ---------------------------------------------------------------------------
// Condition evaluation — pure, no I/O
// ---------------------------------------------------------------------------

/// Returns `Some((confidence, reason))` when `condition` matches `ctx`,
/// or `None` when it does not.
fn evaluate_condition(
    condition: &RuleCondition,
    ctx: &CopilotContext,
) -> Option<(f64, String)> {
    match condition {
        RuleCondition::AppIs(app) => {
            matches_app_is(app, ctx)
        }
        RuleCondition::WindowTitleContains(substr) => {
            matches_window_title(substr, ctx)
        }
        RuleCondition::ClipboardContains(substr) => {
            matches_clipboard(substr, ctx)
        }
        RuleCondition::IdleFor(threshold_ms) => {
            matches_idle(*threshold_ms, ctx)
        }
        RuleCondition::RepeatedAction(action, min_count) => {
            matches_repeated_action(action, *min_count, ctx)
        }
        RuleCondition::Pattern(pattern) => {
            matches_pattern(pattern, ctx)
        }
    }
}

fn matches_app_is(app: &str, ctx: &CopilotContext) -> Option<(f64, String)> {
    if ctx.current_app.to_lowercase() == app.to_lowercase() {
        Some((1.0, format!("Active app is '{}'", ctx.current_app)))
    } else {
        None
    }
}

fn matches_window_title(substr: &str, ctx: &CopilotContext) -> Option<(f64, String)> {
    if ctx.current_window_title.to_lowercase().contains(&substr.to_lowercase()) {
        Some((0.9, format!("Window title contains '{substr}'")))
    } else {
        None
    }
}

fn matches_clipboard(substr: &str, ctx: &CopilotContext) -> Option<(f64, String)> {
    let clip = ctx.clipboard.as_deref()?;
    if clip.to_lowercase().contains(&substr.to_lowercase()) {
        Some((0.85, format!("Clipboard contains '{substr}'")))
    } else {
        None
    }
}

fn matches_idle(threshold_ms: u64, ctx: &CopilotContext) -> Option<(f64, String)> {
    if ctx.time_in_current_app_ms >= threshold_ms {
        Some((
            0.7,
            format!(
                "Idle for {}ms (threshold {threshold_ms}ms)",
                ctx.time_in_current_app_ms,
            ),
        ))
    } else {
        None
    }
}

fn matches_repeated_action(
    action: &str,
    min_count: u32,
    ctx: &CopilotContext,
) -> Option<(f64, String)> {
    let count = u32::try_from(
        ctx.recent_actions
            .iter()
            .filter(|a| a.to_lowercase() == action.to_lowercase())
            .count(),
    )
    .unwrap_or(u32::MAX);
    if count >= min_count {
        Some((
            0.8,
            format!("Action '{action}' repeated {count}×"),
        ))
    } else {
        None
    }
}

fn matches_pattern(pattern: &[String], ctx: &CopilotContext) -> Option<(f64, String)> {
    if pattern.is_empty() {
        return None;
    }
    let actions = &ctx.recent_actions;
    if actions.len() < pattern.len() {
        return None;
    }
    let tail = &actions[actions.len() - pattern.len()..];
    let matched = tail
        .iter()
        .zip(pattern.iter())
        .all(|(a, p)| a.to_lowercase() == p.to_lowercase());
    if matched {
        Some((0.95, format!("Action pattern {pattern:?} detected")))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Built-in rules
// ---------------------------------------------------------------------------

/// Default ruleset installed by [`DesktopCopilot::with_builtin_rules`].
///
/// Provides sensible out-of-the-box heuristics:
/// - Clipboard URL detection
/// - Idle-time reminder
/// - Repeated copy action suggestion
///
/// # Example
///
/// ```
/// use axterminator::copilot::builtin_rules;
///
/// let rules = builtin_rules();
/// assert!(!rules.is_empty());
/// ```
#[must_use]
pub fn builtin_rules() -> Vec<CopilotRule> {
    vec![
        CopilotRule {
            name: "clipboard_url_open_browser".to_owned(),
            condition: RuleCondition::ClipboardContains("http".to_owned()),
            action: SuggestedAction::OpenApp("Safari".to_owned()),
            priority: 80,
        },
        CopilotRule {
            name: "idle_suggest_break".to_owned(),
            condition: RuleCondition::IdleFor(300_000), // 5 minutes
            action: SuggestedAction::ShowTip(
                "You have been in this app for 5 minutes — take a short break?".to_owned(),
            ),
            priority: 30,
        },
        CopilotRule {
            name: "repeated_copy_suggest_snippet".to_owned(),
            condition: RuleCondition::RepeatedAction("copy".to_owned(), 3),
            action: SuggestedAction::ShowTip(
                "You copied this 3 times — consider saving it as a snippet.".to_owned(),
            ),
            priority: 60,
        },
    ]
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers -----------------------------------------------------------

    fn ctx_with_app(app: &str) -> CopilotContext {
        CopilotContext {
            current_app: app.to_owned(),
            current_window_title: String::new(),
            recent_actions: vec![],
            clipboard: None,
            time_in_current_app_ms: 0,
        }
    }

    fn app_rule(app: &str, priority: u8) -> CopilotRule {
        CopilotRule {
            name: format!("rule_{app}"),
            condition: RuleCondition::AppIs(app.to_owned()),
            action: SuggestedAction::ShowTip(format!("tip for {app}")),
            priority,
        }
    }

    // -- AppIs -----------------------------------------------------------

    #[test]
    fn app_is_rule_triggers_on_exact_match() {
        // GIVEN: copilot with an AppIs rule for "VS Code"
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(app_rule("VS Code", 50));
        copilot.update_context(ctx_with_app("VS Code"));

        // WHEN
        let suggestions = copilot.evaluate();

        // THEN: one suggestion produced
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].rule_name, "rule_VS Code");
    }

    #[test]
    fn app_is_rule_is_case_insensitive() {
        // GIVEN: rule registered as "safari", context uses "Safari"
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(app_rule("safari", 50));
        copilot.update_context(ctx_with_app("Safari"));

        // WHEN / THEN: still matches
        assert_eq!(copilot.evaluate().len(), 1);
    }

    #[test]
    fn app_is_rule_does_not_trigger_for_different_app() {
        // GIVEN: rule for "Xcode", context is "Finder"
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(app_rule("Xcode", 50));
        copilot.update_context(ctx_with_app("Finder"));

        // WHEN / THEN
        assert!(copilot.evaluate().is_empty());
    }

    // -- WindowTitleContains ---------------------------------------------

    #[test]
    fn window_title_rule_triggers_on_substring() {
        // GIVEN: rule fires when title contains "PR"
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "pr_review".to_owned(),
            condition: RuleCondition::WindowTitleContains("PR".to_owned()),
            action: SuggestedAction::ShowTip("reviewing a PR".to_owned()),
            priority: 70,
        });
        let mut ctx = ctx_with_app("VS Code");
        ctx.current_window_title = "Fix bug — PR #42".to_owned();
        copilot.update_context(ctx);

        // WHEN / THEN
        let s = copilot.evaluate();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].rule_name, "pr_review");
    }

    #[test]
    fn window_title_rule_does_not_trigger_on_absent_substring() {
        // GIVEN: rule fires when title contains "TODO"
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "todo".to_owned(),
            condition: RuleCondition::WindowTitleContains("TODO".to_owned()),
            action: SuggestedAction::ShowTip("you have a TODO".to_owned()),
            priority: 40,
        });
        let mut ctx = ctx_with_app("Finder");
        ctx.current_window_title = "Documents".to_owned();
        copilot.update_context(ctx);

        // WHEN / THEN
        assert!(copilot.evaluate().is_empty());
    }

    // -- IdleFor ---------------------------------------------------------

    #[test]
    fn idle_rule_triggers_after_threshold_exceeded() {
        // GIVEN: idle threshold = 1 000 ms, context idle = 2 000 ms
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "take_break".to_owned(),
            condition: RuleCondition::IdleFor(1_000),
            action: SuggestedAction::ShowTip("Take a break".to_owned()),
            priority: 50,
        });
        let mut ctx = ctx_with_app("Any");
        ctx.time_in_current_app_ms = 2_000;
        copilot.update_context(ctx);

        // WHEN / THEN
        assert_eq!(copilot.evaluate().len(), 1);
    }

    #[test]
    fn idle_rule_does_not_trigger_before_threshold() {
        // GIVEN: threshold = 5 000 ms, idle = 4 999 ms
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "idle".to_owned(),
            condition: RuleCondition::IdleFor(5_000),
            action: SuggestedAction::ShowTip("idle".to_owned()),
            priority: 50,
        });
        let mut ctx = ctx_with_app("Any");
        ctx.time_in_current_app_ms = 4_999;
        copilot.update_context(ctx);

        // WHEN / THEN
        assert!(copilot.evaluate().is_empty());
    }

    // -- RepeatedAction --------------------------------------------------

    #[test]
    fn repeated_action_triggers_at_min_count() {
        // GIVEN: need 3 copies
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "snippet_hint".to_owned(),
            condition: RuleCondition::RepeatedAction("copy".to_owned(), 3),
            action: SuggestedAction::ShowTip("save as snippet".to_owned()),
            priority: 60,
        });
        let mut ctx = ctx_with_app("VS Code");
        ctx.recent_actions = vec!["copy".to_owned(), "copy".to_owned(), "copy".to_owned()];
        copilot.update_context(ctx);

        // WHEN / THEN
        assert_eq!(copilot.evaluate().len(), 1);
    }

    #[test]
    fn repeated_action_does_not_trigger_below_min_count() {
        // GIVEN: need 3, only have 2
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "snippet_hint".to_owned(),
            condition: RuleCondition::RepeatedAction("copy".to_owned(), 3),
            action: SuggestedAction::ShowTip("save as snippet".to_owned()),
            priority: 60,
        });
        let mut ctx = ctx_with_app("VS Code");
        ctx.recent_actions = vec!["copy".to_owned(), "paste".to_owned(), "copy".to_owned()];
        copilot.update_context(ctx);

        // WHEN / THEN
        assert!(copilot.evaluate().is_empty());
    }

    // -- Pattern ---------------------------------------------------------

    #[test]
    fn pattern_rule_triggers_when_recent_actions_end_with_pattern() {
        // GIVEN: pattern = ["open", "edit", "save"]
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "edit_cycle".to_owned(),
            condition: RuleCondition::Pattern(vec![
                "open".to_owned(),
                "edit".to_owned(),
                "save".to_owned(),
            ]),
            action: SuggestedAction::ShowTip("run tests?".to_owned()),
            priority: 75,
        });
        let mut ctx = ctx_with_app("VS Code");
        ctx.recent_actions = vec![
            "focus".to_owned(),
            "open".to_owned(),
            "edit".to_owned(),
            "save".to_owned(),
        ];
        copilot.update_context(ctx);

        // WHEN / THEN
        assert_eq!(copilot.evaluate().len(), 1);
    }

    #[test]
    fn pattern_rule_does_not_trigger_when_tail_does_not_match() {
        // GIVEN: pattern = ["open", "save"] but last actions are ["edit", "save"]
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "open_save".to_owned(),
            condition: RuleCondition::Pattern(vec!["open".to_owned(), "save".to_owned()]),
            action: SuggestedAction::ShowTip("hint".to_owned()),
            priority: 50,
        });
        let mut ctx = ctx_with_app("VS Code");
        ctx.recent_actions = vec!["edit".to_owned(), "save".to_owned()];
        copilot.update_context(ctx);

        // WHEN / THEN
        assert!(copilot.evaluate().is_empty());
    }

    // -- Priority sorting ------------------------------------------------

    #[test]
    fn suggestions_sorted_by_priority_descending() {
        // GIVEN: two rules with different priorities both matching
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(CopilotRule {
            name: "low_prio".to_owned(),
            condition: RuleCondition::AppIs("App".to_owned()),
            action: SuggestedAction::ShowTip("low".to_owned()),
            priority: 10,
        });
        copilot.add_rule(CopilotRule {
            name: "high_prio".to_owned(),
            condition: RuleCondition::AppIs("App".to_owned()),
            action: SuggestedAction::ShowTip("high".to_owned()),
            priority: 90,
        });
        copilot.update_context(ctx_with_app("App"));

        // WHEN
        let suggestions = copilot.evaluate();

        // THEN: high-priority first
        assert_eq!(suggestions.len(), 2);
        assert_eq!(suggestions[0].rule_name, "high_prio");
        assert_eq!(suggestions[1].rule_name, "low_prio");
    }

    // -- No match --------------------------------------------------------

    #[test]
    fn no_suggestions_when_no_rules_match() {
        // GIVEN: rule for "Terminal", context is "Finder"
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(app_rule("Terminal", 50));
        copilot.update_context(ctx_with_app("Finder"));

        // WHEN / THEN
        assert!(copilot.evaluate().is_empty());
    }

    #[test]
    fn empty_copilot_returns_no_suggestions() {
        // GIVEN: no rules registered
        let mut copilot = DesktopCopilot::new();
        copilot.update_context(ctx_with_app("Safari"));

        // WHEN / THEN
        assert!(copilot.evaluate().is_empty());
    }

    // -- Clipboard URL detection (builtin rule) ---------------------------

    #[test]
    fn builtin_clipboard_url_rule_triggers_on_http_url() {
        // GIVEN: copilot with builtin rules, clipboard contains a URL
        let mut copilot = DesktopCopilot::with_builtin_rules();
        let mut ctx = ctx_with_app("Finder");
        ctx.clipboard = Some("https://example.com/path?q=1".to_owned());
        copilot.update_context(ctx);

        // WHEN
        let suggestions = copilot.evaluate();

        // THEN: the clipboard URL rule fires
        assert!(suggestions
            .iter()
            .any(|s| s.rule_name == "clipboard_url_open_browser"));
    }

    #[test]
    fn builtin_clipboard_url_rule_does_not_trigger_without_http() {
        // GIVEN: clipboard contains plain text
        let mut copilot = DesktopCopilot::with_builtin_rules();
        let mut ctx = ctx_with_app("Finder");
        ctx.clipboard = Some("just some text".to_owned());
        copilot.update_context(ctx);

        // WHEN
        let suggestions = copilot.evaluate();

        // THEN: URL rule absent (other builtin rules may still fire for idle)
        assert!(!suggestions
            .iter()
            .any(|s| s.rule_name == "clipboard_url_open_browser"));
    }

    // -- Suggestion fields -----------------------------------------------

    #[test]
    fn suggestion_confidence_is_in_unit_interval() {
        // GIVEN: any matching rule
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(app_rule("Safari", 50));
        copilot.update_context(ctx_with_app("Safari"));

        // WHEN
        let suggestions = copilot.evaluate();

        // THEN: confidence is in [0, 1]
        for s in &suggestions {
            assert!((0.0..=1.0).contains(&s.confidence));
        }
    }

    #[test]
    fn suggestion_reason_is_non_empty() {
        // GIVEN: any matching rule
        let mut copilot = DesktopCopilot::new();
        copilot.add_rule(app_rule("Terminal", 50));
        copilot.update_context(ctx_with_app("Terminal"));

        // WHEN
        let suggestions = copilot.evaluate();

        // THEN: reason string is set
        assert!(!suggestions[0].reason.is_empty());
    }

    // -- builtin_rules() -------------------------------------------------

    #[test]
    fn builtin_rules_returns_non_empty_slice() {
        // GIVEN / WHEN / THEN
        assert!(!builtin_rules().is_empty());
    }

    #[test]
    fn builtin_rules_all_have_non_empty_names() {
        // GIVEN / WHEN / THEN
        for rule in builtin_rules() {
            assert!(!rule.name.is_empty(), "rule name must not be empty");
        }
    }
}
