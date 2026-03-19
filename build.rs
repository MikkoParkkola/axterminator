fn main() {
    // macOS frameworks needed for accessibility API
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=AppKit");

    // The `python-ext` feature gates `extension-module` in pyo3, which emits
    // `-undefined dynamic_lookup` so the cdylib resolves Python symbols at
    // runtime via the embedding interpreter.
    //
    // When `python-ext` is *disabled* (i.e. building the standalone CLI binary),
    // we must link libpython explicitly so the linker can resolve Python symbols.
    //
    // Cargo exposes enabled features as `CARGO_FEATURE_<UPPER>` env vars inside
    // build scripts, so this detection is reliable across all targets in the crate.
    let python_ext = std::env::var("CARGO_FEATURE_PYTHON_EXT").is_ok();

    if python_ext {
        // Python extension (.so / .dylib): delegate symbol resolution to the
        // embedding interpreter via -undefined dynamic_lookup.
        pyo3_build_config::add_extension_module_link_args();
    } else {
        // Standalone binary: link libpython explicitly.
        let config = pyo3_build_config::get();
        if let Some(lib_dir) = config.lib_dir.as_ref() {
            println!("cargo:rustc-link-search=native={lib_dir}");
        }
        if let Some(lib_name) = config.lib_name.as_ref() {
            println!("cargo:rustc-link-lib=dylib={lib_name}");
        }
    }
}
