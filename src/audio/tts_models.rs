//! Model registry and external runtime bridge for enhanced TTS engines.
//!
//! The large model artifacts are never bundled into the binary.  This module
//! only stores small model metadata, checks the local model cache, and invokes
//! `sherpa-onnx-offline-tts` when the user explicitly requests a non-system
//! TTS engine.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::{AudioError, TtsEngine};

const KOKORO_REQUIRED_FILES: &[&str] = &[
    "model.onnx",
    "voices.bin",
    "tokens.txt",
    "espeak-ng-data",
    "lexicon-us-en.txt",
];

const PIPER_REQUIRED_FILES: &[&str] = &["en_US-lessac-medium.onnx", "tokens.txt", "espeak-ng-data"];

const KOKORO_SPEAKERS: &[(&str, u16)] = &[
    ("af_alloy", 0),
    ("af_aoede", 1),
    ("af_bella", 2),
    ("af_heart", 3),
    ("af_jessica", 4),
    ("af_kore", 5),
    ("af_nicole", 6),
    ("af_nova", 7),
    ("af_river", 8),
    ("af_sarah", 9),
    ("af_sky", 10),
    ("am_adam", 11),
    ("am_echo", 12),
    ("am_eric", 13),
    ("am_fenrir", 14),
    ("am_liam", 15),
    ("am_michael", 16),
    ("am_onyx", 17),
    ("am_puck", 18),
    ("am_santa", 19),
    ("bf_alice", 20),
    ("bf_emma", 21),
    ("bf_isabella", 22),
    ("bf_lily", 23),
    ("bm_daniel", 24),
    ("bm_fable", 25),
    ("bm_george", 26),
    ("bm_lewis", 27),
    ("ef_dora", 28),
    ("em_alex", 29),
    ("ff_siwis", 30),
    ("hf_alpha", 31),
    ("hf_beta", 32),
    ("hm_omega", 33),
    ("hm_psi", 34),
    ("if_sara", 35),
    ("im_nicola", 36),
    ("jf_alpha", 37),
    ("jf_gongitsune", 38),
    ("jf_nezumi", 39),
    ("jf_tebukuro", 40),
    ("jm_kumo", 41),
    ("pf_dora", 42),
    ("pm_alex", 43),
    ("pm_santa", 44),
    ("zf_xiaobei", 45),
    ("zf_xiaoni", 46),
    ("zf_xiaoxiao", 47),
    ("zf_xiaoyi", 48),
    ("zm_yunjian", 49),
    ("zm_yunxi", 50),
    ("zm_yunxia", 51),
    ("zm_yunyang", 52),
];

/// Static metadata for an enhanced TTS model bundle.
#[derive(Debug, Clone, Copy)]
pub struct TtsModelSpec {
    pub engine: TtsEngine,
    pub display_name: &'static str,
    pub directory: &'static str,
    pub archive_url: &'static str,
    pub docs_url: &'static str,
    pub runtime_binary: &'static str,
    pub required_files: &'static [&'static str],
    pub description: &'static str,
}

/// Local installation status for a TTS model bundle.
#[derive(Debug, Clone)]
pub struct TtsModelStatus {
    pub spec: &'static TtsModelSpec,
    pub path: PathBuf,
    pub installed: bool,
    pub missing_files: Vec<String>,
}

/// Result returned by the model downloader.
#[derive(Debug, Clone)]
pub struct TtsModelDownload {
    pub spec: &'static TtsModelSpec,
    pub path: PathBuf,
    pub downloaded: bool,
}

/// Model bundles supported by the enhanced TTS feature.
pub const TTS_MODEL_SPECS: &[TtsModelSpec] = &[
    TtsModelSpec {
        engine: TtsEngine::Kokoro,
        display_name: "Kokoro 82M multilingual",
        directory: "kokoro",
        archive_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2",
        docs_url: "https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html",
        runtime_binary: "sherpa-onnx-offline-tts",
        required_files: KOKORO_REQUIRED_FILES,
        description: "Natural local neural TTS with multi-speaker voice IDs.",
    },
    TtsModelSpec {
        engine: TtsEngine::Piper,
        display_name: "Piper en_US lessac medium",
        directory: "piper",
        archive_url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/vits-piper-en_US-lessac-medium.tar.bz2",
        docs_url: "https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/vits.html",
        runtime_binary: "sherpa-onnx-offline-tts",
        required_files: PIPER_REQUIRED_FILES,
        description: "Lightweight local Piper voice packaged for sherpa-onnx.",
    },
];

/// Resolve the model cache root.
///
/// Enhanced TTS models are stored under `~/.axterminator/models/tts/`.
pub fn model_root() -> Result<PathBuf, AudioError> {
    let home = std::env::var("HOME").map_err(|_| {
        AudioError::Synthesis("Cannot determine $HOME for TTS model directory".to_string())
    })?;
    Ok(PathBuf::from(home)
        .join(".axterminator")
        .join("models")
        .join("tts"))
}

/// Return the static model spec for an enhanced TTS engine.
#[must_use]
pub fn spec_for_engine(engine: TtsEngine) -> Option<&'static TtsModelSpec> {
    TTS_MODEL_SPECS.iter().find(|spec| spec.engine == engine)
}

/// Return the local model directory for an enhanced TTS engine.
pub fn model_dir(engine: TtsEngine) -> Result<PathBuf, AudioError> {
    let spec = spec_for_engine(engine).ok_or_else(|| {
        AudioError::Synthesis(format!(
            "{} does not use the enhanced TTS model cache",
            engine.as_str()
        ))
    })?;
    Ok(model_root()?.join(spec.directory))
}

/// Return local installation status for each enhanced TTS engine.
pub fn model_statuses() -> Result<Vec<TtsModelStatus>, AudioError> {
    TTS_MODEL_SPECS
        .iter()
        .map(|spec| model_status_for_engine(spec.engine))
        .collect()
}

/// Return local installation status for one enhanced TTS engine.
pub fn model_status_for_engine(engine: TtsEngine) -> Result<TtsModelStatus, AudioError> {
    let spec = spec_for_engine(engine).ok_or_else(|| {
        AudioError::Synthesis(format!("Unknown enhanced TTS engine: {}", engine.as_str()))
    })?;
    let path = model_dir(engine)?;
    let missing_files = missing_model_files_in_dir(&path, spec);
    Ok(TtsModelStatus {
        spec,
        path,
        installed: missing_files.is_empty(),
        missing_files,
    })
}

/// Return true when all required model files are present.
pub fn model_files_present(engine: TtsEngine) -> bool {
    model_status_for_engine(engine)
        .map(|status| status.installed)
        .unwrap_or(false)
}

/// Return the missing file names for an engine's model bundle.
pub fn missing_model_files(engine: TtsEngine) -> Result<Vec<String>, AudioError> {
    Ok(model_status_for_engine(engine)?.missing_files)
}

/// Download and extract an enhanced TTS model bundle.
///
/// The download uses the system `curl` and `tar` commands to avoid adding a
/// network stack to the default binary.  The command is only reachable when the
/// `enhanced-tts` feature is compiled in.
pub fn download_model(engine: TtsEngine, force: bool) -> Result<TtsModelDownload, AudioError> {
    let spec = spec_for_engine(engine).ok_or_else(|| {
        AudioError::Synthesis(format!(
            "Engine {} has no downloadable TTS model",
            engine.as_str()
        ))
    })?;
    let root = model_root()?;
    let target_dir = root.join(spec.directory);

    if target_dir.exists() && model_files_present(engine) && !force {
        return Ok(TtsModelDownload {
            spec,
            path: target_dir,
            downloaded: false,
        });
    }

    if force && target_dir.exists() {
        std::fs::remove_dir_all(&target_dir).map_err(|e| {
            AudioError::Synthesis(format!("Failed to remove {}: {e}", target_dir.display()))
        })?;
    }

    std::fs::create_dir_all(&target_dir).map_err(|e| {
        AudioError::Synthesis(format!("Failed to create {}: {e}", target_dir.display()))
    })?;

    let archive_path = root.join(format!("{}.tar.bz2.download", spec.directory));
    let curl_status = Command::new("curl")
        .args(["--fail", "--location", "--show-error", "--output"])
        .arg(&archive_path)
        .arg(spec.archive_url)
        .status()
        .map_err(|e| AudioError::Synthesis(format!("Failed to start curl: {e}")))?;
    if !curl_status.success() {
        return Err(AudioError::Synthesis(format!(
            "curl failed while downloading {} from {}",
            spec.display_name, spec.archive_url
        )));
    }

    let tar_status = Command::new("tar")
        .arg("-xf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&target_dir)
        .arg("--strip-components=1")
        .status()
        .map_err(|e| AudioError::Synthesis(format!("Failed to start tar: {e}")))?;
    let _ = std::fs::remove_file(&archive_path);

    if !tar_status.success() {
        return Err(AudioError::Synthesis(format!(
            "tar failed while extracting {} into {}",
            spec.display_name,
            target_dir.display()
        )));
    }

    let status = model_status_for_engine(engine)?;
    if !status.installed {
        return Err(AudioError::Synthesis(format!(
            "{} extracted but required files are missing: {}",
            spec.display_name,
            status.missing_files.join(", ")
        )));
    }

    Ok(TtsModelDownload {
        spec,
        path: target_dir,
        downloaded: true,
    })
}

/// Synthesize speech with `sherpa-onnx-offline-tts` and play it through `afplay`.
pub fn speak_with_sherpa(
    text: &str,
    voice: Option<&str>,
    engine: TtsEngine,
) -> Result<Duration, AudioError> {
    let missing = missing_model_files(engine)?;
    if !missing.is_empty() {
        return Err(AudioError::Synthesis(format!(
            "{} model files not downloaded (missing: {}). Run \
             `axterminator models tts download {}` first.",
            engine.as_str(),
            missing.join(", "),
            engine.as_str()
        )));
    }

    let output = tempfile::Builder::new()
        .prefix("axterminator-tts-")
        .suffix(".wav")
        .tempfile()
        .map_err(|e| AudioError::Synthesis(format!("Failed to create temp WAV: {e}")))?;

    let started = Instant::now();
    let synth_output = build_sherpa_command(engine, voice, text, output.path())?
        .output()
        .map_err(|e| {
            AudioError::Synthesis(format!(
                "Failed to start {}. Install it or set AXTERMINATOR_SHERPA_ONNX_TTS: {e}",
                sherpa_binary()
            ))
        })?;

    if !synth_output.status.success() {
        let stderr = String::from_utf8_lossy(&synth_output.stderr);
        return Err(AudioError::Synthesis(format!(
            "{} failed for {}: {}",
            sherpa_binary(),
            engine.as_str(),
            stderr.trim()
        )));
    }

    let play_status = Command::new("afplay")
        .arg(output.path())
        .status()
        .map_err(|e| AudioError::Synthesis(format!("Failed to start afplay: {e}")))?;
    if !play_status.success() {
        return Err(AudioError::Synthesis(format!(
            "afplay failed while playing {} output",
            engine.as_str()
        )));
    }

    Ok(started.elapsed())
}

fn missing_model_files_in_dir(path: &Path, spec: &TtsModelSpec) -> Vec<String> {
    spec.required_files
        .iter()
        .filter(|file| !path.join(file).exists())
        .map(|file| (*file).to_string())
        .collect()
}

fn build_sherpa_command(
    engine: TtsEngine,
    voice: Option<&str>,
    text: &str,
    output_path: &Path,
) -> Result<Command, AudioError> {
    let dir = model_dir(engine)?;
    let mut cmd = Command::new(sherpa_binary());
    cmd.arg("--num-threads=2")
        .arg(format!("--output-filename={}", output_path.display()));

    match engine {
        TtsEngine::Kokoro => {
            let speaker = kokoro_speaker_id(voice)?;
            let lexicons = [dir.join("lexicon-us-en.txt"), dir.join("lexicon-zh.txt")]
                .iter()
                .filter(|path| path.exists())
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(",");
            cmd.arg(format!(
                "--kokoro-model={}",
                dir.join("model.onnx").display()
            ))
            .arg(format!(
                "--kokoro-voices={}",
                dir.join("voices.bin").display()
            ))
            .arg(format!(
                "--kokoro-tokens={}",
                dir.join("tokens.txt").display()
            ))
            .arg(format!(
                "--kokoro-data-dir={}",
                dir.join("espeak-ng-data").display()
            ))
            .arg(format!("--sid={speaker}"));
            if !lexicons.is_empty() {
                cmd.arg(format!("--kokoro-lexicon={lexicons}"));
            }
        }
        TtsEngine::Piper => {
            cmd.arg(format!(
                "--vits-model={}",
                dir.join("en_US-lessac-medium.onnx").display()
            ))
            .arg(format!(
                "--vits-tokens={}",
                dir.join("tokens.txt").display()
            ))
            .arg(format!(
                "--vits-data-dir={}",
                dir.join("espeak-ng-data").display()
            ));
        }
        TtsEngine::System => {
            return Err(AudioError::Synthesis(
                "system TTS does not use sherpa-onnx".to_string(),
            ));
        }
    }

    cmd.arg(text);
    Ok(cmd)
}

fn sherpa_binary() -> String {
    std::env::var("AXTERMINATOR_SHERPA_ONNX_TTS")
        .unwrap_or_else(|_| "sherpa-onnx-offline-tts".to_string())
}

fn kokoro_speaker_id(voice: Option<&str>) -> Result<u16, AudioError> {
    let Some(candidate) = voice.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(3);
    };

    if let Ok(id) = candidate.parse::<u16>()
        && KOKORO_SPEAKERS
            .iter()
            .any(|(_, speaker_id)| *speaker_id == id)
    {
        return Ok(id);
    }

    KOKORO_SPEAKERS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(candidate))
        .map(|(_, id)| *id)
        .ok_or_else(|| {
            AudioError::Synthesis(format!(
                "Unknown Kokoro voice \"{candidate}\". Use one of {} or a speaker id 0-52.",
                KOKORO_SPEAKERS
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_lookup_excludes_system_engine() {
        assert!(spec_for_engine(TtsEngine::System).is_none());
        assert_eq!(
            spec_for_engine(TtsEngine::Kokoro).unwrap().directory,
            "kokoro"
        );
        assert_eq!(
            spec_for_engine(TtsEngine::Piper).unwrap().directory,
            "piper"
        );
    }

    #[test]
    fn kokoro_voice_accepts_default_name_and_numeric_id() {
        assert_eq!(kokoro_speaker_id(None).unwrap(), 3);
        assert_eq!(kokoro_speaker_id(Some("af_heart")).unwrap(), 3);
        assert_eq!(kokoro_speaker_id(Some("27")).unwrap(), 27);
    }

    #[test]
    fn kokoro_voice_rejects_unknown_name() {
        let err = kokoro_speaker_id(Some("does_not_exist")).unwrap_err();
        assert_eq!(err.code(), "synthesis_error");
    }
}
