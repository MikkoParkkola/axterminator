//! Instruction and observation source precedence for GUI automation.
//!
//! The policy is intentionally small and deterministic so MCP handlers can
//! apply it before handing control to lower-trust visual fallback paths.

/// Environment variable controlling whether source priority is enforced.
pub const PRIORITY_MODE_ENV: &str = "AXTERMINATOR_PRIORITY_MODE";

/// Runtime mode for instruction/source priority behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcePriorityMode {
    /// Preserve historical visual lookup behavior.
    Legacy,
    /// Enforce explicit instruction and UI fact precedence.
    Explicit,
}

impl SourcePriorityMode {
    /// Read the priority mode from `AXTERMINATOR_PRIORITY_MODE`.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_raw(std::env::var(PRIORITY_MODE_ENV).ok().as_deref())
    }

    /// Parse a mode value. Unknown or unset values default to legacy mode as a
    /// conservative rollback default.
    #[must_use]
    pub fn from_raw(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some("explicit") => Self::Explicit,
            Some("legacy") | None => Self::Legacy,
            Some(_) => Self::Legacy,
        }
    }

    /// Stable lowercase identifier used in JSON responses and docs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Explicit => "explicit",
        }
    }

    /// Whether explicit priority should be enforced.
    #[must_use]
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::Explicit)
    }
}

/// Source of an instruction or UI observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionSource {
    /// The initiating human user's prompt.
    HumanUserPrompt,
    /// Tool arguments supplied by an agent on the user's behalf.
    AgentToolArgs,
    /// Tool arguments supplied directly by a human caller.
    HumanToolArgs,
    /// Facts returned by the macOS Accessibility API.
    AxApi,
    /// Facts exposed by a first-party application dialog.
    AppDialog,
    /// Facts inferred from screenshot or vision model output.
    ScreenVision,
}

impl InstructionSource {
    /// Stable lowercase identifier used in JSON responses and docs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HumanUserPrompt => "human_user_prompt",
            Self::AgentToolArgs => "agent_tool_args",
            Self::HumanToolArgs => "human_tool_args",
            Self::AxApi => "ax_api",
            Self::AppDialog => "app_dialog",
            Self::ScreenVision => "screen_vision",
        }
    }
}

/// Caller class for MCP tool invocations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationActor {
    /// A human directly supplied the tool arguments.
    Human,
    /// An agent supplied tool arguments while acting for a human.
    Agent,
}

impl InvocationActor {
    /// Parse an MCP `caller` argument. Unknown values default to `Agent`,
    /// because agent-mediated calls are the higher-risk path.
    #[must_use]
    pub fn from_tool_arg(raw: Option<&str>) -> Self {
        match raw {
            Some("human") => Self::Human,
            Some("agent") | None => Self::Agent,
            Some(_) => Self::Agent,
        }
    }

    /// Stable lowercase identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
        }
    }
}

/// A candidate value from one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCandidate {
    /// Candidate source.
    pub source: InstructionSource,
    /// Candidate value.
    pub value: String,
}

impl SourceCandidate {
    /// Build a candidate value.
    #[must_use]
    pub fn new(source: InstructionSource, value: impl Into<String>) -> Self {
        Self {
            source,
            value: value.into(),
        }
    }
}

/// Chosen value and provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDecision {
    /// Source that won precedence.
    pub source: InstructionSource,
    /// Value to use downstream.
    pub value: String,
    /// Source that was overridden, when there was a real conflict.
    pub overridden_source: Option<InstructionSource>,
    /// Human-readable reason for audit output.
    pub reason: &'static str,
}

impl SourceDecision {
    /// Whether the effective field differs from lower-priority tool args.
    #[must_use]
    pub const fn overrode_tool_args(&self) -> bool {
        matches!(
            self.overridden_source,
            Some(InstructionSource::AgentToolArgs)
        )
    }
}

/// Choose the effective element description for a visual fallback request.
#[must_use]
pub fn select_effective_description(
    tool_description: &str,
    user_prompt: Option<&str>,
    actor: InvocationActor,
) -> SourceDecision {
    let tool_value = tool_description.trim();
    let prompt_value = user_prompt.map(str::trim).filter(|value| !value.is_empty());

    match (actor, prompt_value) {
        (InvocationActor::Agent, Some(prompt)) => {
            let conflicts = !same_instruction(prompt, tool_value);
            SourceDecision {
                source: InstructionSource::HumanUserPrompt,
                value: prompt.to_string(),
                overridden_source: conflicts.then_some(InstructionSource::AgentToolArgs),
                reason: if conflicts {
                    "human user prompt has higher authority than agent-supplied tool args"
                } else {
                    "human user prompt and agent tool args agree"
                },
            }
        }
        (InvocationActor::Agent, None) => SourceDecision {
            source: InstructionSource::AgentToolArgs,
            value: tool_value.to_string(),
            overridden_source: None,
            reason: "no separate initiating user prompt was supplied",
        },
        (InvocationActor::Human, _) => SourceDecision {
            source: InstructionSource::HumanToolArgs,
            value: tool_value.to_string(),
            overridden_source: None,
            reason: "direct human tool args are treated as the user instruction",
        },
    }
}

/// Preserve historical behavior while still producing provenance metadata.
#[must_use]
pub fn select_legacy_description(tool_description: &str, actor: InvocationActor) -> SourceDecision {
    SourceDecision {
        source: match actor {
            InvocationActor::Human => InstructionSource::HumanToolArgs,
            InvocationActor::Agent => InstructionSource::AgentToolArgs,
        },
        value: tool_description.trim().to_string(),
        overridden_source: None,
        reason: "legacy mode preserves the supplied tool description",
    }
}

/// Choose the highest-priority UI fact for a single field.
#[must_use]
pub fn select_ui_fact(candidates: &[SourceCandidate]) -> Option<SourceDecision> {
    let chosen = candidates
        .iter()
        .filter(|candidate| !candidate.value.trim().is_empty())
        .max_by_key(|candidate| ui_fact_rank(candidate.source))?;

    let overridden_source = candidates
        .iter()
        .filter(|candidate| candidate.source != chosen.source)
        .filter(|candidate| !candidate.value.trim().is_empty())
        .filter(|candidate| !same_instruction(&candidate.value, &chosen.value))
        .max_by_key(|candidate| ui_fact_rank(candidate.source))
        .map(|candidate| candidate.source);

    Some(SourceDecision {
        source: chosen.source,
        value: chosen.value.trim().to_string(),
        overridden_source,
        reason: match chosen.source {
            InstructionSource::AxApi => {
                "AX API facts have higher authority than app-dialog or screen-vision facts"
            }
            InstructionSource::AppDialog => {
                "app-dialog facts have higher authority than screen-vision facts when AX is absent"
            }
            InstructionSource::ScreenVision => {
                "screen vision is used only when no higher UI fact exists"
            }
            InstructionSource::HumanUserPrompt
            | InstructionSource::AgentToolArgs
            | InstructionSource::HumanToolArgs => {
                "instruction sources are not UI facts; preserve supplied order"
            }
        },
    })
}

fn ui_fact_rank(source: InstructionSource) -> u8 {
    match source {
        InstructionSource::AxApi => 30,
        InstructionSource::AppDialog => 20,
        InstructionSource::ScreenVision => 10,
        InstructionSource::HumanUserPrompt
        | InstructionSource::AgentToolArgs
        | InstructionSource::HumanToolArgs => 0,
    }
}

fn same_instruction(left: &str, right: &str) -> bool {
    normalise(left) == normalise(right)
}

fn normalise(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ax_api_beats_screen_vision_for_same_field() {
        let chosen = select_ui_fact(&[
            SourceCandidate::new(InstructionSource::ScreenVision, "Delete"),
            SourceCandidate::new(InstructionSource::AxApi, "Cancel"),
        ])
        .expect("candidate selected");

        assert_eq!(chosen.source, InstructionSource::AxApi);
        assert_eq!(chosen.value, "Cancel");
        assert_eq!(
            chosen.overridden_source,
            Some(InstructionSource::ScreenVision)
        );
    }

    #[test]
    fn app_dialog_beats_screen_vision_when_ax_is_missing() {
        let chosen = select_ui_fact(&[
            SourceCandidate::new(InstructionSource::ScreenVision, "OK"),
            SourceCandidate::new(InstructionSource::AppDialog, "Don't Save"),
        ])
        .expect("candidate selected");

        assert_eq!(chosen.source, InstructionSource::AppDialog);
        assert_eq!(chosen.value, "Don't Save");
    }

    #[test]
    fn user_prompt_overrides_agent_tool_args() {
        let chosen = select_effective_description(
            "click the delete button",
            Some("click the cancel button"),
            InvocationActor::Agent,
        );

        assert_eq!(chosen.source, InstructionSource::HumanUserPrompt);
        assert_eq!(chosen.value, "click the cancel button");
        assert!(chosen.overrode_tool_args());
    }

    #[test]
    fn human_direct_tool_args_are_the_user_instruction() {
        let chosen = select_effective_description(
            "click the delete button",
            Some("click the cancel button"),
            InvocationActor::Human,
        );

        assert_eq!(chosen.source, InstructionSource::HumanToolArgs);
        assert_eq!(chosen.value, "click the delete button");
        assert!(!chosen.overrode_tool_args());
    }

    #[test]
    fn priority_mode_defaults_to_legacy() {
        assert_eq!(
            SourcePriorityMode::from_raw(None),
            SourcePriorityMode::Legacy
        );
        assert_eq!(
            SourcePriorityMode::from_raw(Some("unknown")),
            SourcePriorityMode::Legacy
        );
    }

    #[test]
    fn priority_mode_accepts_explicit_and_legacy() {
        assert_eq!(
            SourcePriorityMode::from_raw(Some("explicit")),
            SourcePriorityMode::Explicit
        );
        assert_eq!(
            SourcePriorityMode::from_raw(Some("legacy")),
            SourcePriorityMode::Legacy
        );
    }
}
