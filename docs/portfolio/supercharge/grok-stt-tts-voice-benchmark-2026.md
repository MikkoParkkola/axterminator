# MIK-3313 Research Memo: Grok STT/TTS Voice Benchmark and Fallback

Evidence date: 2026-05-12

## Decision

Use xAI Grok STT/TTS as the portfolio voice benchmark of record for cloud
speech quality, but keep AXTerminator's default audio path local and
privacy-preserving. Grok should be a conditional cloud fallback, not a default
runtime dependency.

Current AXTerminator audio routing is local:

- `ax_listen` captures microphone or system audio and can transcribe with the
  `apple` or `parakeet` engines.
- `ax_speak` synthesizes speech with the `system`, `kokoro`, or `piper`
  engines.
- The audio module documents that speech recognition and synthesis are
  on-device/local when those features are used.

Recommendation:

- Position Grok STT as the cloud benchmark leader on xAI-published phone-call
  entity WER.
- Treat the xAI benchmark table as provider-published evidence until reproduced
  on an AXTerminator-owned fixture set.
- Treat the ElevenLabs 12.0% WER number as sourced from xAI's comparison table,
  not from an ElevenLabs-published benchmark.
- Require explicit user or deployment opt-in before sending audio or generated
  speech text to xAI.

## Acceptance Criteria

- MIK-3313.RESEARCH.1: xAI Grok STT API access is assessed below with endpoint,
  auth model, pricing, rate limits, language support, and integration notes.
- MIK-3313.RESEARCH.2: Grok TTS API access is assessed below with endpoint, auth
  model, pricing, rate limits, voice selection, output formats, and integration
  notes.
- MIK-3313.RESEARCH.3: the 5.0% WER claim is validated as an xAI-published
  benchmark claim; the ElevenLabs 12.0% WER reference is sourced to xAI's
  comparison table and marked as provider-comparative, not ElevenLabs SSOT.
- MIK-3313.RESEARCH.4: Speech Tags prosody pattern is documented with inline
  tags, wrapping tags, supported effects, and the proposed AXTerminator
  integration surface.
- MIK-3313.RESEARCH.5: AXTerminator conditional cloud fallback is sketched with
  trigger conditions, call path, privacy guardrails, and latency budget.
- MIK-3313.RESEARCH.6: this memo updates the portfolio voice benchmark of record
  and defines the current cloud STT/TTS positioning.

## MIK-3313.RESEARCH.1 - Grok STT API Access

Current public xAI documentation exposes Grok Speech to Text through product
endpoints rather than an OpenAI-compatible `/audio/transcriptions` endpoint.

| Field | Finding |
| --- | --- |
| REST endpoint | `POST https://api.x.ai/v1/stt` |
| Streaming endpoint | `wss://api.x.ai/v1/stt` |
| Auth model | Bearer API key in `Authorization: Bearer $XAI_API_KEY` |
| Request body | `multipart/form-data`; provide `file` or `url`; optional `language`, `format`, `multichannel`, `diarize`, `filler_words`, `audio_format`, `sample_rate`, and `channels` |
| Response | Transcript text, detected language, duration, word-level timing, optional channel and speaker fields |
| Pricing | REST batch: `$0.10 / hr`; streaming: `$0.20 / hr` |
| Rate limits | REST: 600 RPM and 10 RPS. Streaming: 600 RPM, 10 RPS, and 100 concurrent sessions per team |
| Region | `us-east-1` in the model page |
| Languages | Public docs list broad multilingual support including English, Spanish, French, German, Portuguese, Hindi, Japanese, Korean, Polish, Czech, Finnish, Swedish, and others |
| Notable features | Word-level timestamps, multichannel transcription, speaker diarization, filler-word control, and inverse text normalization when `format=true` |

Access assessment:

- The API is usable only with an xAI API key and should be treated as a closed
  cloud provider dependency.
- No live API call was run in this research pass because no `XAI_API_KEY` was
  available in the environment.
- AXTerminator should prefer the streaming endpoint only for interactive
  sessions where interim results matter; file-based fallback can use REST.

## MIK-3313.RESEARCH.2 - Grok TTS API Access

Current public xAI documentation exposes Grok Text to Speech through `/v1/tts`.

| Field | Finding |
| --- | --- |
| REST endpoint | `POST https://api.x.ai/v1/tts` |
| Streaming endpoint | `wss://api.x.ai/v1/tts` |
| Auth model | Bearer API key in `Authorization: Bearer $XAI_API_KEY` |
| Request body | JSON with `text`, `voice_id`, `language`, optional `output_format`, `stream`, `optimize_streaming_latency`, and text normalization controls |
| WebSocket config | Query parameters include `language`, `voice`, `codec`, `sample_rate`, `bit_rate`, `optimize_streaming_latency`, and `text_normalization` |
| Voice selection | Documented voices are `ara`, `eve`, `leo`, `rex`, and `sal`; REST uses `voice_id`, WebSocket uses `voice` |
| Output formats | MP3, WAV, PCM, mu-law, and A-law; default MP3 at 24 kHz / 128 kbps |
| Pricing | `$15.00 / 1M characters` on the current xAI announcement and model page |
| Rate limits | Model page: 3,000 RPM, 50 RPS, and 100 concurrent sessions per team. The TTS guide separately states 50 concurrent WebSocket sessions per team |
| Region | `us-east-1` in the model page |
| Limits | Unary/server-streamed `POST /v1/tts` accepts up to 15,000 characters per request; the bidirectional WebSocket has no total text limit, but each `text.delta` is capped at 15,000 characters |

Access assessment:

- Voice choice is explicit enough for an AXTerminator fallback surface.
- Public docs do not currently expose voice cloning for this endpoint; use fixed
  xAI voices only.
- API console should be treated as the rate-limit SSOT before implementation
  because product docs and guide pages distinguish model-level concurrency from
  WebSocket session concurrency.

## MIK-3313.RESEARCH.3 - Benchmark Validation

The 5.0% WER claim is validated as xAI-published public evidence. xAI's launch
post includes an enterprise transcription table where "Phone Call Entities"
shows:

| Provider | Phone Call Entities WER |
| --- | --- |
| Grok STT | 5.0% |
| ElevenLabs | 12.0% |
| Deepgram | 13.5% |
| AssemblyAI | 21.3% |

The same xAI table reports Grok at 6.9% overall WER and ElevenLabs at 9.0%
overall WER across the displayed domains.

Evidence caveat:

- The 12.0% ElevenLabs reference is sourced to xAI's comparison table, not to an
  ElevenLabs-published benchmark page.
- ElevenLabs' current docs describe Scribe v2 as state-of-the-art and publish
  language buckets such as "Excellent (<= 5% WER)" rather than the xAI phone-call
  entity benchmark number.
- The portfolio claim should therefore read: "xAI publishes Grok STT at 5.0%
  WER on phone-call entities versus ElevenLabs at 12.0% in xAI's comparison
  table." It should not read: "ElevenLabs says it has 12.0% WER."

## MIK-3313.RESEARCH.4 - Speech Tags Prosody Pattern

Speech Tags are xAI's TTS prosody control surface. They use two markup shapes:

1. Inline tags that insert an expression at the natural point in the sentence.
2. Wrapping tags that alter the delivery style of enclosed text.

Documented inline tags:

| Category | Tags |
| --- | --- |
| Pauses | `[pause]`, `[long-pause]`, `[hum-tune]` |
| Laughter and crying | `[laugh]`, `[chuckle]`, `[giggle]`, `[cry]` |
| Mouth sounds | `[tsk]`, `[tongue-click]`, `[lip-smack]` |
| Breathing | `[breath]`, `[inhale]`, `[exhale]`, `[sigh]` |

Documented wrapping tags:

| Category | Tags |
| --- | --- |
| Volume and intensity | `<soft>`, `<whisper>`, `<loud>`, `<build-intensity>`, `<decrease-intensity>` |
| Pitch and speed | `<higher-pitch>`, `<lower-pitch>`, `<slow>`, `<fast>` |
| Vocal style | `<sing-song>`, `<singing>`, `<laugh-speak>`, `<emphasis>` |

Integration surface:

```json
{
  "engine": "grok",
  "voice_id": "eve",
  "text": "I need to tell you something. <whisper>It is a secret.</whisper>",
  "prosody_tags": ["<whisper>"],
  "output_format": {"codec": "mp3", "sample_rate": 24000}
}
```

AXTerminator should not expose arbitrary markup injection as an unvalidated
string helper. The runtime provider adapter should:

- Keep tags in the text field because xAI's API consumes inline/wrapping tags
  there.
- Validate tags against an allowlist before sending requests.
- Reject mismatched wrapping tags.
- Include `prosody_tags` in trace metadata for reproducibility and debugging.
- Strip or escape Speech Tags when falling back to local `system`, `kokoro`, or
  `piper` engines unless those engines gain an equivalent prosody contract.

## MIK-3313.RESEARCH.5 - AXTerminator Conditional Cloud Fallback

Fallback should be opt-in and provider-neutral. Proposed configuration:

```toml
[audio.cloud_fallback]
enabled = true
provider = "xai"
api_key_env = "XAI_API_KEY"
allow_sensitive_audio = false
stt_timeout_ms = 750
tts_first_audio_budget_ms = 300
```

Trigger conditions:

- Primary local STT or TTS engine returns an error.
- Requested language is unsupported by the selected local engine but supported
  by Grok.
- User explicitly requests cloud quality or Speech Tags prosody.
- Local STT returns low confidence once AXTerminator has a confidence-bearing
  transcription result type.
- Operator policy allows cloud fallback for the current app, workflow, and data
  sensitivity class.

STT call path:

1. `ax_listen` captures microphone or system audio.
2. Provider router tries `apple` or `parakeet` first.
3. On allowed fallback, encode the captured audio and call `POST /v1/stt` or
   stream frames to `wss://api.x.ai/v1/stt`.
4. Return transcript plus provenance: `engine_used = "grok-stt"`,
   `provider = "xai"`, `cloud = true`, `formatted = true|false`, and timing
   metrics.

TTS call path:

1. `ax_speak` tries `system`, `kokoro`, or `piper` first.
2. If local synthesis fails, the requested voice is unavailable, or Speech Tags
   are required, route to xAI only when cloud fallback is enabled.
3. Call `POST /v1/tts` for normal utterances or `wss://api.x.ai/v1/tts` for
   interactive sessions.
4. Decode and play returned audio, recording `engine_used = "grok-tts"` and
   output codec/sample rate.

Latency budget:

| Segment | Budget | Rationale |
| --- | --- | --- |
| Local primary attempt before fallback | 750 ms | Avoid making fallback feel hung while still giving local engines a chance |
| STT streaming interim result | 500 ms | xAI docs expose interim streaming results; AXTerminator should target sub-second partial feedback |
| TTS first audio | 300 ms | xAI markets low-latency TTS; product budget should leave playback overhead around the provider latency |
| Short-turn cloud voice loop | 1,200 ms | Practical upper bound for command-response interactions |

Privacy and safety guardrails:

- Cloud fallback must be disabled by default.
- Do not send audio from password fields, payment flows, private documents, or
  user-blocklisted applications.
- Record provenance in every transcript or speech result so downstream tools can
  tell local and cloud outputs apart.
- Cache generated TTS only when user policy allows it; never cache raw STT audio
  by default.

## MIK-3313.RESEARCH.6 - Portfolio Voice Benchmark of Record

Portfolio voice benchmark of record as of 2026-05-12:

| Rank | Provider/path | Status | Benchmark position | AXTerminator role |
| --- | --- | --- | --- | --- |
| 1 | Grok STT/TTS | Closed cloud API | xAI-published 5.0% WER on phone-call entities, Speech Tags for expressive TTS | Conditional cloud fallback and quality benchmark |
| 2 | ElevenLabs Scribe | Closed cloud API | xAI comparison table reports 12.0% WER on phone-call entities; ElevenLabs docs publish broader `<= 5% WER` language buckets for Scribe v1/v2 support | Competitive reference, not AXTerminator default |
| 3 | Parakeet | Optional local engine | No AXTerminator-owned benchmark yet | Best local STT candidate once measured |
| 4 | Apple SFSpeechRecognizer | Built-in local engine | No AXTerminator-owned WER benchmark yet | Default privacy-preserving STT baseline |
| 5 | system/Kokoro/Piper | Local TTS engines | No AXTerminator-owned MOS/prosody benchmark yet | Default privacy-preserving TTS baseline |

Next benchmark slice:

1. Build a small AXTerminator-owned voice fixture set: command utterances,
   UI labels, names, numbers, dates, and noisy system-audio clips.
2. Measure WER and entity error rate for Apple, Parakeet, Grok, and ElevenLabs
   on the same fixture.
3. Measure TTS first-audio latency, full utterance latency, and subjective
   prosody quality for system, Kokoro, Piper, Grok, and ElevenLabs.
4. Promote Grok from benchmark-of-record candidate to implementation target only
   after API-key access, privacy policy, and measured fixture results pass.

## Sources

- xAI Grok STT/TTS announcement: https://x.ai/news/grok-stt-and-tts-apis
- xAI Speech to Text guide: https://docs.x.ai/developers/model-capabilities/audio/speech-to-text
- xAI Text to Speech guide: https://docs.x.ai/developers/model-capabilities/audio/text-to-speech
- xAI Speech to Text model page: https://docs.x.ai/developers/models/speech-to-text
- xAI Text to Speech model page: https://docs.x.ai/developers/models/text-to-speech
- ElevenLabs Speech to Text docs: https://elevenlabs.io/docs/overview/capabilities/speech-to-text/
- ElevenLabs Speech to Text help page: https://help.elevenlabs.io/hc/en-us/articles/33053029255697-What-is-Speech-to-Text
- AXTerminator audio guide: [audio.md](../../guide/audio.md)
- AXTerminator MCP audio tools: [mcp-tools.md](../../api/mcp-tools.md)
