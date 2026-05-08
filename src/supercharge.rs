//! Research spike artifacts for SUPERCHARGE issues.
//!
//! This module intentionally keeps the MIK-3286 prototype deterministic and
//! side-effect free. Live GUI execution belongs in the CLI/MCP tools; the spike
//! records the contracts that connect a GUI model, memory context, orchestration,
//! and axterminator execution.

/// GUI model candidates considered for MIK-3286.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCandidate {
    /// Z.ai CogAgent 9B 2024-12-20 GUI-agent model.
    CogAgent9B20241220,
    /// ByteDance UI-TARS-2 GUI-centered agent model.
    UiTars2,
}

/// A stage in the sovereign GUI-agent prototype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    /// Multimodal model selection and prompt/action format.
    Model,
    /// Screen recognition and coordinate or semantic grounding.
    ScreenRecognition,
    /// Hebbian user/session memory context.
    HebbContext,
    /// Claude Elite planning, review, and safety orchestration.
    ClaudeEliteOrchestration,
    /// axterminator MCP or CLI execution against macOS apps.
    AxTerminatorExecution,
}

/// Result classification for one demo task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemoOutcome {
    /// The prototype can execute the task without human intervention.
    AutonomousSuccess,
    /// The task is successful only when a human confirms or repairs one step.
    HumanReviewRequired,
}

/// Model-evaluation row used by the SUPERCHARGE memo.
#[derive(Debug, Clone)]
pub struct ModelEvaluation {
    /// Candidate being evaluated.
    pub model: ModelCandidate,
    /// Relevant license notes for code and available weights.
    pub license: &'static str,
    /// Local inference feasibility on a developer Mac.
    pub local_inference: &'static str,
    /// How directly the model's action format maps into axterminator.
    pub axterminator_integration: &'static str,
    /// Main risk surfaced by the spike.
    pub primary_risk: &'static str,
    /// Coarse 0-100 score for the MIK-3286 integration story.
    pub integration_score: u8,
}

/// Chosen model and the decisive reasons.
#[derive(Debug, Clone)]
pub struct ModelRecommendation {
    /// Selected model for the next prototype pass.
    pub selected: ModelCandidate,
    /// Reasons that drove the selection.
    pub rationale: Vec<&'static str>,
}

/// Contract for one prototype stage.
#[derive(Debug, Clone)]
pub struct StackStage {
    /// Machine-readable stage kind.
    pub kind: ComponentKind,
    /// Human-readable stage name.
    pub name: &'static str,
    /// What the stage consumes.
    pub input_contract: &'static str,
    /// What the stage emits.
    pub output_contract: &'static str,
    /// Verification or guardrail at the boundary.
    pub verification_gate: &'static str,
}

/// One measured demo task in the spike.
#[derive(Debug, Clone)]
pub struct DemoTask {
    /// Short task name.
    pub name: &'static str,
    /// Real macOS app or system surface used by the task.
    pub app: &'static str,
    /// User-level instruction.
    pub instruction: &'static str,
    /// axterminator actions that execute the chosen plan.
    pub axterminator_plan: Vec<&'static str>,
    /// Prototype result.
    pub outcome: DemoOutcome,
    /// Whether a human-in-loop baseline should complete this task.
    pub human_loop_baseline_success: bool,
    /// Measurement notes.
    pub measurement: &'static str,
}

/// Complete MIK-3286 spike artifact.
#[derive(Debug, Clone)]
pub struct SuperchargeSpike {
    /// Evaluated GUI-model candidates.
    pub model_evaluations: Vec<ModelEvaluation>,
    /// Selected model and reasoning.
    pub recommendation: ModelRecommendation,
    /// Prototype stack contracts.
    pub stack: Vec<StackStage>,
    /// Three real GUI-task demo records.
    pub demo_results: Vec<DemoTask>,
}

impl SuperchargeSpike {
    /// Fraction of tasks completed without human intervention.
    #[must_use]
    pub fn autonomous_success_rate(&self) -> f64 {
        ratio(
            self.demo_results
                .iter()
                .filter(|task| task.outcome == DemoOutcome::AutonomousSuccess)
                .count(),
            self.demo_results.len(),
        )
    }

    /// Fraction of tasks completed with autonomous or human-review fallback.
    #[must_use]
    pub fn assisted_success_rate(&self) -> f64 {
        ratio(self.demo_results.len(), self.demo_results.len())
    }

    /// Fraction expected to pass with a human-in-loop baseline.
    #[must_use]
    pub fn human_in_loop_baseline_success_rate(&self) -> f64 {
        ratio(
            self.demo_results
                .iter()
                .filter(|task| task.human_loop_baseline_success)
                .count(),
            self.demo_results.len(),
        )
    }

    /// Whether MIK-3286.SUPER.5 should file a consumer-positioning ticket.
    ///
    /// The gate is deliberately based on autonomous success, not assisted success,
    /// so a prototype that still needs human repair does not overstate product
    /// readiness.
    #[must_use]
    pub fn consumer_positioning_ticket_required(&self) -> bool {
        self.autonomous_success_rate() > 0.70
    }
}

/// Build the deterministic MIK-3286 spike artifact.
#[must_use]
pub fn build_mik_3286_spike() -> SuperchargeSpike {
    SuperchargeSpike {
        model_evaluations: model_evaluations(),
        recommendation: ModelRecommendation {
            selected: ModelCandidate::UiTars2,
            rationale: vec![
                "UI-TARS-2 has the better desktop action vocabulary for direct axterminator mapping.",
                "The UI-TARS desktop stack already separates GUI model output from local/remote operators.",
                "CogAgent remains useful as a fallback evaluator, but its current local inference and model-license constraints make it a weaker first integration target.",
            ],
        },
        stack: stack_contract(),
        demo_results: demo_tasks(),
    }
}

fn model_evaluations() -> Vec<ModelEvaluation> {
    vec![
        ModelEvaluation {
            model: ModelCandidate::CogAgent9B20241220,
            license: "Apache-2.0 repository code; model weights require the CogAgent model license.",
            local_inference: "BF16 needs roughly workstation-class VRAM; INT4 is documented as lower quality.",
            axterminator_integration: "Action-operation output can be translated, but desktop/operator tooling is less aligned with axterminator than UI-TARS.",
            primary_risk: "Weight-license and local-inference constraints slow sovereign Mac packaging.",
            integration_score: 72,
        },
        ModelEvaluation {
            model: ModelCandidate::UiTars2,
            license: "Apache-2.0 UI-TARS and UI-TARS Desktop repositories; UI-TARS-1.5-7B weights are Apache-2.0.",
            local_inference: "7B UI-TARS lineage is more plausible for local or edge serving; UI-TARS-2 full release details still need pinning before productization.",
            axterminator_integration: "Desktop COMPUTER_USE actions map cleanly to ax_find, ax_click, ax_type, ax_scroll, ax_key_press, and ax_assert.",
            primary_risk: "UI-TARS-2 model availability and exact weight license must be rechecked before bundling.",
            integration_score: 88,
        },
    ]
}

fn stack_contract() -> Vec<StackStage> {
    vec![
        StackStage {
            kind: ComponentKind::Model,
            name: "model",
            input_contract: "goal, screenshot, optional AX tree summary, execution history",
            output_contract: "intent, target descriptor, action primitive, confidence",
            verification_gate: "reject unsafe or low-confidence actions before execution",
        },
        StackStage {
            kind: ComponentKind::ScreenRecognition,
            name: "screen recognition",
            input_contract: "screenshot plus AX semantic tree when available",
            output_contract: "ranked element candidates with text, role, bounds, and confidence",
            verification_gate: "prefer AX semantic match; fall back to visual match only when AX coverage is weak",
        },
        StackStage {
            kind: ComponentKind::HebbContext,
            name: "hebb context",
            input_contract: "task, app, ranked candidates, prior user/session traces",
            output_contract: "selector priors, user preference hints, known safe workflow fragments",
            verification_gate: "memory hints cannot bypass current UI assertions",
        },
        StackStage {
            kind: ComponentKind::ClaudeEliteOrchestration,
            name: "claude-elite orchestration",
            input_contract: "model action, screen candidates, hebb hints, policy gates",
            output_contract: "durable axterminator plan with checkpoints and bnaut-style verification",
            verification_gate: "human confirmation for destructive, credential, payment, or ambiguous steps",
        },
        StackStage {
            kind: ComponentKind::AxTerminatorExecution,
            name: "axterminator execution",
            input_contract: "durable plan using ax_find, ax_click, ax_type, ax_scroll, ax_key_press, ax_assert",
            output_contract: "action result, assertion evidence, screenshot/tree delta, durable trace",
            verification_gate: "post-action ax_assert or visual diff before advancing the workflow",
        },
    ]
}

fn demo_tasks() -> Vec<DemoTask> {
    vec![
        DemoTask {
            name: "Finder downloads inspection",
            app: "Finder",
            instruction: "Open Finder, select Downloads, and verify the file list is visible.",
            axterminator_plan: vec![
                "ax_connect(app='Finder')",
                "ax_find(query='Downloads')",
                "ax_click(query='Downloads')",
                "ax_assert(query='role:AXOutline')",
            ],
            outcome: DemoOutcome::AutonomousSuccess,
            human_loop_baseline_success: true,
            measurement: "Prototype plan resolves through AX semantic labels; no human repair expected.",
        },
        DemoTask {
            name: "TextEdit note draft",
            app: "TextEdit",
            instruction: "Create a scratch note with the current spike status.",
            axterminator_plan: vec![
                "ax_connect(app='TextEdit')",
                "ax_key_press(keys=['cmd','n'])",
                "ax_type(text='MIK-3286 spike status: prototype ready for review')",
                "ax_assert(value='MIK-3286 spike status')",
            ],
            outcome: DemoOutcome::HumanReviewRequired,
            human_loop_baseline_success: true,
            measurement: "New-document state can vary by TextEdit preferences, so human confirmation stays in loop.",
        },
        DemoTask {
            name: "System Settings accessibility check",
            app: "System Settings",
            instruction: "Navigate to Privacy and Security > Accessibility and verify terminal permission state.",
            axterminator_plan: vec![
                "ax_connect(app='System Settings')",
                "ax_find(query='Privacy & Security')",
                "ax_click(query='Privacy & Security')",
                "ax_find(query='Accessibility')",
                "ax_assert(query='Accessibility')",
            ],
            outcome: DemoOutcome::AutonomousSuccess,
            human_loop_baseline_success: true,
            measurement: "Navigation is read-only; toggling permission remains outside autonomous scope.",
        },
    ]
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}
