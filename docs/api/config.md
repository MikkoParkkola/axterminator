# Configuration

AXTerminator is configured through environment variables, CLI flags on
`axterminator mcp serve`, and an optional security policy file. There is no
configuration API: the Python bindings that once carried one were removed in
v0.7.0.

## Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `AXTERMINATOR_SECURITY_MODE` | Security mode: `normal`, `safe`, or `sandboxed` | `normal` |
| `AXTERMINATOR_RATE_LIMIT_RPS` | Tool calls allowed per second (sliding one-second window) | `50` |
| `AXTERMINATOR_PRIORITY_MODE` | Visual lookup source-priority mode: `legacy` or `explicit` | `legacy` |
| `AXTERMINATOR_HTTP_TOKEN` | Bearer token for the HTTP transport (same as `--token`; needs the `http-transport` feature) | random token generated at startup and printed to stderr |
| `AXTERMINATOR_LLM_ENDPOINT` | Optional endpoint that re-ranks the top structural candidates during semantic find | unset, structural score used directly |
| `AXTERMINATOR_SHERPA_ONNX_TTS` | Path or name of the sherpa-onnx offline TTS binary (needs the `enhanced-tts` feature) | `sherpa-onnx-offline-tts` |

## Security Modes

| Mode | Behaviour |
|------|-----------|
| `normal` (default) | All tools allowed; mutating calls are logged. |
| `safe` | Scripting tools blocked; `tools/list` reflects the restriction. |
| `sandboxed` | Read-only tools only; every write returns a policy error. |

## App Policy File

`~/.config/axterminator/security.toml` is optional. It holds `allowed` and
`denied` arrays of app names or bundle IDs. If the file is absent, every app is
allowed.

```toml
allowed = ["Calculator", "com.apple.Safari"]
denied  = ["com.apple.Keychain-Access", "1Password"]
```

## Audit Log

Every mutating tool call is appended to
`~/.local/share/axterminator/audit.jsonl` as one JSON line:

```json
{"ts":"2025-11-05T12:00:00Z","tool":"ax_click","args":{},"result":"ok"}
```

## Self-Healing Configuration

`HealingConfig` is a Rust library type (`axterminator::HealingConfig`) with the
fields `strategies`, `max_heal_time_ms` (default `100`) and `cache_healed`
(default `true`). It is not wired into the MCP server, so there is nothing to
configure when you run `axterminator mcp serve`. See
[Self-Healing](../guide/self-healing.md).
