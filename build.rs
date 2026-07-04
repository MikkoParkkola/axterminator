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

    // Work around an ort-sys 2.0.0-rc.12 bug on Xcode 26+:
    // ort-sys resolves the clang library dir via `clang --print-search-dirs` and
    // emits `{dir}/lib/darwin` as a linker search path without checking whether
    // that subdirectory actually exists.  On Xcode 26+ the resource dir uses the
    // major-only version name (e.g. `.../clang/21`) while `libclang_rt.osx.a`
    // lives in the full-version sibling (e.g. `.../clang/21.0.0/lib/darwin`),
    // causing `ld: library 'clang_rt.osx' not found`.
    //
    // We emit the correct search path from our build script so the linker finds
    // `libclang_rt.osx.a` even when ort-sys points at the wrong directory.
    fix_clang_rt_search_path();

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

/// Emit a supplementary `cargo:rustc-link-search` for `libclang_rt.osx.a` on
/// macOS when the path normally used by ort-sys does not exist.
///
/// ### Background
/// `ort-sys ≤ 2.0.0-rc.12` calls `clang --print-search-dirs`, extracts the
/// `libraries:` path (e.g. `.../clang/21`), and unconditionally emits
/// `{dir}/lib/darwin` as a linker search path plus `cargo:rustc-link-lib=clang_rt.osx`.
/// It does *not* verify that `{dir}/lib/darwin` exists.
///
/// On Xcode 26+ (macOS 26 / Tahoe) Apple ships the clang resource directory
/// with only the major version component (`.../clang/21`) but places the
/// actual runtime libraries inside the full-version sibling directory
/// (`.../clang/21.0.0/lib/darwin`).  The `lib/darwin` subdirectory of the
/// major-only path therefore does not exist, and the linker fails with
/// `ld: library 'clang_rt.osx' not found`.
///
/// ### Fix
/// We scan the clang toolchain directory for any sibling that contains a
/// `lib/darwin` subdirectory and, if found, emit it as an additional
/// `cargo:rustc-link-search`.  The linker then searches both the (missing)
/// ort-sys path and our correct path, finds `libclang_rt.osx.a` in the
/// latter, and the link succeeds.
///
/// On Xcode ≤ 25 the `lib/darwin` directory already exists at the expected
/// location, so this function emits nothing and is a no-op.
fn fix_clang_rt_search_path() {
    // Only applicable on macOS.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    // Re-run if the active developer directory changes (e.g. xcode-select).
    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");

    let Ok(output) = std::process::Command::new("clang")
        .arg("--print-search-dirs")
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // Match the absolute-path variant: "libraries: =/some/path"
        // Use find() rather than strip_prefix() to tolerate leading whitespace.
        let Some(eq_pos) = line.find("libraries: =") else {
            continue;
        };
        let path = line[eq_pos + "libraries: =".len()..].trim();
        if path.is_empty() {
            continue;
        }

        let lib_darwin = std::path::Path::new(path).join("lib/darwin");
        if lib_darwin.is_dir() {
            // The expected path exists; ort-sys will handle it correctly.
            return;
        }

        // `lib/darwin` is missing at the major-only path.  Scan sibling
        // directories (e.g. "21.0.0") for the real `lib/darwin` tree.
        let parent = std::path::Path::new(path)
            .parent()
            .unwrap_or(std::path::Path::new("/"));
        if let Ok(entries) = std::fs::read_dir(parent) {
            let mut candidates: Vec<_> = entries
                .flatten()
                .map(|e| e.path().join("lib/darwin"))
                .filter(|p| p.is_dir())
                .collect();
            // Sort so we pick the highest version when multiple exist.
            candidates.sort();
            if let Some(found) = candidates.last() {
                println!("cargo:rustc-link-search=native={}", found.display());
            }
        }

        // Only process the first "libraries:" line.
        break;
    }
}
