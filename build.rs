fn main() {
    // PyO3 handles Python linking automatically when building extension modules
    // We just need to inform cargo about the pyo3 build configuration
    pyo3_build_config::add_extension_module_link_args();

    // macOS frameworks needed for accessibility API
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=AppKit");

    // XPC is part of libSystem on macOS - no explicit linking needed
    // The functions are resolved at runtime from libSystem.B.dylib
}
