use std::fs;

#[test]
fn mik_3227_memo_records_ui_tars_supercharge_decision() {
    let memo = fs::read_to_string("docs/portfolio/supercharge/ui-tars-2-axterminator-2026.md")
        .expect("MIK-3227 memo should exist");

    for needle in [
        "MIK-3227.SUPER.1",
        "MIK-3227.SUPER.2",
        "MIK-3227.SUPER.3",
        "MIK-3227.SUPER.4",
        "MIK-3227.SUPER.5",
        "UI-TARS-2",
        "230B total",
        "23B active",
        "OSWorld",
        "WindowsAgentArena",
        "AndroidWorld",
        "Online-Mind2Web",
        "AX-first With Vision Fallback",
        "nvfp4-mojo",
        "hebb",
        "sub-3GB",
        "UI-TARS-2B-SFT",
    ] {
        assert!(memo.contains(needle), "memo missing {needle}");
    }
}
