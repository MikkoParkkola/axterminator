use std::fs;

#[test]
fn mik_3285_memo_records_runtime_fit_and_acceptance_criteria() {
    let memo = fs::read_to_string("docs/portfolio/supercharge/glm-5v-autoclaw-runtime-2026.md")
        .expect("MIK-3285 memo should exist");

    for needle in [
        "MIK-3285.RESEARCH.1",
        "MIK-3285.RESEARCH.2",
        "MIK-3285.RESEARCH.3",
        "MIK-3285.RESEARCH.4",
        "MIK-3285.RESEARCH.5",
        "GLM-5V-Turbo",
        "AutoClaw",
        "Claude Code",
        "AXTerminator",
        "ADR-0001",
        "model brain plus semantic hands",
        "provider-neutral GLM-style perception adapter",
    ] {
        assert!(memo.contains(needle), "memo missing {needle}");
    }
}
