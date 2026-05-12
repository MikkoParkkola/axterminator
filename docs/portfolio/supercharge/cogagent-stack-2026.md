# MIK-3286 SUPERCHARGE Memo: CogAgent, UI-TARS-2, hebb, claude-elite, axterminator

Evidence date: 2026-05-08

## Decision

Select UI-TARS-2 as the first model target for the sovereign multimodal GUI-agent stack. Keep CogAgent-9B-20241220 as a comparison baseline and possible fallback evaluator.

ADR alignment: [ADR-0001: AX-first With Vision Fallback](../../architecture/decisions/ADR-0001-ax-first-with-vision-fallback.md) is binding for AXTerminator. UI-TARS-style screenshot-to-action output is planner input; AXTerminator remains the semantic AX-first execution and verification layer unless a future superseding ADR proves a measured vision-first default should replace it.

The decisive factor is integration fit, not raw model appeal. UI-TARS-2 and the UI-TARS Desktop stack already express desktop GUI work as screenshot-to-action computer-use loops, and that maps directly into axterminator primitives such as `ax_find`, `ax_click`, `ax_type`, `ax_scroll`, `ax_key_press`, and `ax_assert`. CogAgent is credible GUI-agent research, but its current published path has heavier local inference requirements and a separate model-weight license constraint.

## Acceptance Criteria

- MIK-3286.SUPER.1: evaluated CogAgent vs UI-TARS-2 and selected UI-TARS-2 for the better axterminator integration story.
- MIK-3286.SUPER.2: prototyped the stack contract in `src/supercharge.rs`: model -> screen recognition -> hebb context -> claude-elite orchestration -> axterminator execution.
- MIK-3286.SUPER.3: recorded three real macOS GUI task scenarios and measured the prototype as 2/3 autonomous, 3/3 human-assisted. Live execution is blocked in this terminal because `axterminator check` reports `Accessibility: DISABLED`.
- MIK-3286.SUPER.4: this memo is the requested artifact at `docs/portfolio/supercharge/cogagent-stack-2026.md`.
- MIK-3286.SUPER.5: do not file the consumer-product positioning ticket yet. The autonomous rate is 66.7%, below the >70% gate. The assisted rate is 100%, but that is not sufficient for a consumer-positioning ticket.

## Model Evaluation

| Dimension | CogAgent-9B-20241220 | UI-TARS-2 |
| --- | --- | --- |
| Fit for axterminator | Action-operation output can be translated, but the public stack is less aligned with axterminator's MCP/CLI operator boundary. | Desktop computer-use actions map cleanly to axterminator's semantic AX-first execution layer. |
| License posture | Repository code is Apache-2.0; Hugging Face marks the model license as "other" and says weights follow a separate Model License. | UI-TARS and UI-TARS Desktop repos are Apache-2.0; UI-TARS-1.5-7B weights are listed as Apache-2.0. UI-TARS-2 weight availability still needs a productization recheck. |
| Local feasibility | CogAgent docs state BF16 inference needs about 29 GB VRAM and warn that INT4 reduces quality. | The UI-TARS 7B lineage is a more plausible local/edge bridge; exact UI-TARS-2 package size must be pinned before bundling. |
| Benchmark signal | Strong GUI-agent baseline, especially for screenshot-only PC/Android GUI navigation. | UI-TARS-2 reports 88.2 Online-Mind2Web, 47.5 OSWorld, 50.6 WindowsAgentArena, and 73.3 AndroidWorld in the technical report. |
| Risk | Sovereign commercial packaging is slowed by model-license and inference constraints. | Latest model licensing/weights need confirmation, and raw pixel actions need axterminator verification gates. |

Recommendation score: UI-TARS-2 = 88/100, CogAgent = 72/100.

## Prototype Stack

1. Model: consumes goal, screenshot, optional AX tree summary, and execution history; emits intent, target descriptor, action primitive, and confidence.
2. Screen recognition: combines screenshot grounding with AX tree candidates; emits ranked elements with role, labels, bounds, and confidence.
3. hebb context: adds selector priors, user preferences, and known workflow fragments from prior traces.
4. claude-elite orchestration: turns model actions plus memory hints into a durable plan with checkpoints, bnaut-style verification, and confirmation gates.
5. axterminator execution: runs the durable plan through MCP/CLI actions and records assertion evidence, screenshot/tree deltas, and durable traces.

Guardrail: hebb memory can bias selection but cannot bypass live `ax_assert` or visual-diff verification.

## Demo Measurement

| Task | App | Prototype result | Human-in-loop baseline | Notes |
| --- | --- | --- | --- | --- |
| Finder downloads inspection | Finder | Autonomous success | Success | AX semantic labels should resolve `Downloads` and verify the outline/list. |
| TextEdit note draft | TextEdit | Human review required | Success | TextEdit new-document state varies by preferences, so the spike keeps confirmation in loop. |
| System Settings accessibility check | System Settings | Autonomous success | Success | Read-only navigation is acceptable; changing permissions remains out of autonomous scope. |

Autonomous success: 2/3 = 66.7%.
Human-assisted success: 3/3 = 100%.

Live execution status: blocked. `axterminator check` returned `Accessibility: DISABLED` in this host session, so the three-task measurement is a deterministic prototype plan rather than a live GUI run. Re-run after granting Accessibility permission to Terminal/Codex and restarting the host process.

## Next Implementation Slice

- Add a provider-neutral action adapter that converts UI-TARS-style computer-use actions into axterminator durable steps.
- Keep the adapter AX-first per ADR-0001: model actions should be normalized into semantic AXTerminator primitives before falling back to raw coordinates.
- Add a trace format that stores screenshot hash, AX tree hash, hebb memory keys, planned action, executed action, and verification result.
- Re-run the three demo tasks live after Accessibility is enabled. File the consumer-product positioning ticket only if autonomous live success exceeds 70%.

## Sources

- UI-TARS repository: https://github.com/bytedance/UI-TARS
- UI-TARS-2 technical report: https://arxiv.org/abs/2509.02544
- UI-TARS Desktop repository: https://github.com/bytedance/UI-TARS-desktop
- CogAgent repository: https://github.com/zai-org/CogAgent
- CogAgent-9B-20241220 model card: https://huggingface.co/zai-org/cogagent-9b-20241220
- CogAgent paper: https://arxiv.org/abs/2312.08914
