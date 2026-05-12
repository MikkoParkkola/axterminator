use std::fs;

#[test]
fn mik_3313_memo_records_grok_voice_benchmark_and_fallback() {
    let memo =
        fs::read_to_string("docs/portfolio/supercharge/grok-stt-tts-voice-benchmark-2026.md")
            .expect("MIK-3313 memo should exist");

    for needle in [
        "MIK-3313.RESEARCH.1",
        "MIK-3313.RESEARCH.2",
        "MIK-3313.RESEARCH.3",
        "MIK-3313.RESEARCH.4",
        "MIK-3313.RESEARCH.5",
        "MIK-3313.RESEARCH.6",
        "POST https://api.x.ai/v1/stt",
        "wss://api.x.ai/v1/stt",
        "POST https://api.x.ai/v1/tts",
        "wss://api.x.ai/v1/tts",
        "Authorization: Bearer $XAI_API_KEY",
        "$0.10 / hr",
        "$0.20 / hr",
        "$15.00 / 1M characters",
        "5.0%",
        "12.0%",
        "provider-published evidence",
        "Speech Tags",
        "[pause]",
        "<whisper>",
        "voice_id",
        "ara",
        "eve",
        "leo",
        "rex",
        "sal",
        "conditional cloud fallback",
        "AXTerminator",
        "ax_listen",
        "ax_speak",
        "apple",
        "parakeet",
        "system",
        "kokoro",
        "piper",
        "Portfolio voice benchmark of record",
        "XAI_API_KEY",
    ] {
        assert!(memo.contains(needle), "memo missing {needle}");
    }
}
