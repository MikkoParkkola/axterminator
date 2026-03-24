fn main() {
    // Embed Info.plist into every binary target so CoreLocation (and other
    // TCC-gated frameworks) can display system permission dialogs from CLI
    // tools.  Without an embedded plist, `requestWhenInUseAuthorization`
    // silently returns kCLAuthorizationStatusDenied for CLI tools.
    //
    // `rustc-link-arg-bins` applies only to [[bin]] targets, leaving the
    // rlib clean.  The path is relative to the workspace root at link time.
    let plist = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/Info.plist");
    println!("cargo:rustc-link-arg-bins=-sectcreate");
    println!("cargo:rustc-link-arg-bins=__TEXT");
    println!("cargo:rustc-link-arg-bins=__info_plist");
    println!("cargo:rustc-link-arg-bins={plist}");
    println!("cargo:rerun-if-changed=resources/Info.plist");

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

    // Context feature: link CoreLocation for geolocation.
    if std::env::var("CARGO_FEATURE_CONTEXT").is_ok() {
        println!("cargo:rustc-link-lib=framework=CoreLocation");
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
        // CoreMedia provides CMSampleBuffer processing (used by SCK audio capture).
        println!("cargo:rustc-link-lib=framework=CoreMedia");

        // ScreenCaptureKit: compile the ObjC wrapper and weak-link the framework.
        // Weak linking means the binary still works on macOS 13 (where SCK audio-only
        // is unavailable). At runtime, axt_sck_is_available() checks @available.
        compile_sck_audio_objc();
        // Weak link: resolved at runtime, nil if framework absent.
        println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    }
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

/// Compile `src/sck_audio_objc.m` into a static library that Cargo links.
///
/// The ObjC file wraps ScreenCaptureKit audio-only capture (macOS 14+).
/// Uses @available guards so it compiles on older SDKs but only activates
/// at runtime on macOS 14+.
fn compile_sck_audio_objc() {
    cc::Build::new()
        .file("src/sck_audio_objc.m")
        .flag("-fobjc-arc")
        .flag("-fmodules")
        // Allow use of newer API with @available checks.
        .flag("-Wno-unguarded-availability-new")
        // Target macOS 13+ to compile successfully with SCK headers.
        .flag("-mmacosx-version-min=13.0")
        .compile("sck_audio_objc");

    println!("cargo:rerun-if-changed=src/sck_audio_objc.m");
}
