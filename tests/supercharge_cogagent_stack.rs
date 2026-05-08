use std::fs;

use axterminator::supercharge::{ComponentKind, DemoOutcome, ModelCandidate, build_mik_3286_spike};

#[test]
fn mik_3286_selects_ui_tars_2_for_axterminator_integration() {
    let spike = build_mik_3286_spike();

    assert_eq!(spike.recommendation.selected, ModelCandidate::UiTars2);
    assert!(
        spike
            .recommendation
            .rationale
            .iter()
            .any(|reason| reason.contains("desktop action vocabulary")),
        "recommendation should explain the axterminator integration advantage"
    );
    assert!(
        spike
            .model_evaluations
            .iter()
            .any(|candidate| candidate.model == ModelCandidate::CogAgent9B20241220),
        "CogAgent must be evaluated, not skipped"
    );
}

#[test]
fn mik_3286_prototype_has_required_stack_order() {
    let spike = build_mik_3286_spike();
    let stages: Vec<ComponentKind> = spike.stack.iter().map(|stage| stage.kind).collect();

    assert_eq!(
        stages,
        vec![
            ComponentKind::Model,
            ComponentKind::ScreenRecognition,
            ComponentKind::HebbContext,
            ComponentKind::ClaudeEliteOrchestration,
            ComponentKind::AxTerminatorExecution,
        ]
    );

    assert!(
        spike
            .stack
            .iter()
            .all(|stage| !stage.input_contract.is_empty() && !stage.output_contract.is_empty())
    );
}

#[test]
fn mik_3286_demo_measures_three_real_gui_tasks_against_baseline() {
    let spike = build_mik_3286_spike();

    assert_eq!(spike.demo_results.len(), 3);
    assert!(
        spike
            .demo_results
            .iter()
            .all(|task| !task.app.is_empty() && !task.axterminator_plan.is_empty())
    );
    assert!(
        spike
            .demo_results
            .iter()
            .any(|task| task.outcome == DemoOutcome::HumanReviewRequired),
        "at least one task should preserve human-in-loop fallback"
    );
    assert!(spike.assisted_success_rate() > 0.70);
    assert!(spike.autonomous_success_rate() < spike.assisted_success_rate());
    assert!(spike.human_in_loop_baseline_success_rate() >= spike.assisted_success_rate());
    assert!(!spike.consumer_positioning_ticket_required());
}

#[test]
fn mik_3286_memo_exists_and_records_acceptance_criteria() {
    let memo = fs::read_to_string("docs/portfolio/supercharge/cogagent-stack-2026.md")
        .expect("MIK-3286 memo should exist");

    for needle in [
        "MIK-3286.SUPER.1",
        "MIK-3286.SUPER.2",
        "MIK-3286.SUPER.3",
        "MIK-3286.SUPER.4",
        "MIK-3286.SUPER.5",
        "UI-TARS-2",
        "CogAgent",
        "hebb",
        "axterminator",
    ] {
        assert!(memo.contains(needle), "memo missing {needle}");
    }
}
