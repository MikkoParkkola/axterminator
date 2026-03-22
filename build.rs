fn main() {
    // macOS frameworks needed for accessibility API
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=AppKit");

    // Camera feature: compile the Objective-C AVFoundation + Vision bindings
    // and link the required system frameworks.
    if std::env::var("CARGO_FEATURE_CAMERA").is_ok() {
        compile_camera_objc();
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Vision");
        println!("cargo:rustc-link-lib=framework=CoreImage");
        println!("cargo:rustc-link-lib=framework=ImageIO");
        println!("cargo:rustc-link-lib=framework=CoreMedia");
        println!("cargo:rustc-link-lib=framework=CoreVideo");
    }

    // Audio feature: link CoreAudio, AVFoundation, and Speech frameworks.
    // Conditional to avoid TCC dialogs for users who do not need audio.
    if std::env::var("CARGO_FEATURE_AUDIO").is_ok() {
        // CoreAudio provides AudioObjectGetPropertyData/AudioObjectGetPropertyDataSize.
        println!("cargo:rustc-link-lib=framework=CoreAudio");
        // AudioToolbox provides AudioQueue APIs.
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
        // AVFoundation provides AVAudioEngine, AVCaptureDevice, etc.
        // Duplicate link directives are harmless — the linker deduplicates them.
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        // Speech.framework provides SFSpeechRecognizer.
        println!("cargo:rustc-link-lib=framework=Speech");
    }

    // The `python-ext` feature gates `extension-module` in pyo3, which emits
    // `-undefined dynamic_lookup` so the cdylib resolves Python symbols at
    // runtime via the embedding interpreter.
    //
    // When `python-ext` is *disabled* (i.e. building the standalone CLI binary),
    // pyo3 is not a dependency at all — no Python linking is needed or performed.
    //
    // Cargo exposes enabled features as `CARGO_FEATURE_<UPPER>` env vars inside
    // build scripts, so this detection is reliable across all targets in the crate.
    let python_ext = std::env::var("CARGO_FEATURE_PYTHON_EXT").is_ok();

    if python_ext {
        // Python extension (.so / .dylib): delegate symbol resolution to the
        // embedding interpreter via -undefined dynamic_lookup.
        pyo3_build_config::add_extension_module_link_args();
    }
    // When python-ext is disabled, don't link Python at all.
    // The CLI binary has zero Python dependency.
}

/// Compile `src/camera_objc.m` into a static library that Cargo links.
///
/// Uses the system `cc` tool (clang on macOS) via the `cc` build-dependency.
/// The Objective-C file is only compiled when the `camera` feature is active.
fn compile_camera_objc() {
    cc::Build::new()
        .file("src/camera_objc.m")
        .flag("-fobjc-arc")
        .flag("-fmodules")
        // Suppress the AVCaptureDeviceTypeExternal deprecation on macOS 14+:
        // we fall back to the discovery session API on 10.15+ anyway.
        .flag("-Wno-deprecated-declarations")
        .compile("camera_objc");

    // Tell Cargo to re-run this build script when the .m file changes.
    println!("cargo:rerun-if-changed=src/camera_objc.m");
}
