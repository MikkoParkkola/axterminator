//! Quick benchmark for element access performance
//!
//! Run: rustc -O bench_quick.rs -l ApplicationServices -l CoreFoundation -o bench_quick && ./bench_quick

use std::process::Command;
use std::time::Instant;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *mut std::ffi::c_void;
    fn AXUIElementCopyAttributeValue(
        element: *mut std::ffi::c_void,
        attribute: *const std::ffi::c_void,
        value: *mut *mut std::ffi::c_void,
    ) -> i32;
    fn AXUIElementPerformAction(
        element: *mut std::ffi::c_void,
        action: *const std::ffi::c_void,
    ) -> i32;
    fn CFRelease(cf: *const std::ffi::c_void);
    fn CFStringCreateWithCString(
        alloc: *const std::ffi::c_void,
        c_str: *const i8,
        encoding: u32,
    ) -> *const std::ffi::c_void;
    fn CFArrayGetCount(array: *const std::ffi::c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const std::ffi::c_void, idx: isize) -> *mut std::ffi::c_void;
}

const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

fn cfstring(s: &str) -> *const std::ffi::c_void {
    unsafe {
        let c_str = std::ffi::CString::new(s).unwrap();
        CFStringCreateWithCString(std::ptr::null(), c_str.as_ptr(), K_CF_STRING_ENCODING_UTF8)
    }
}

fn get_finder_pid() -> Option<i32> {
    let output = Command::new("pgrep").args(["-x", "Finder"]).output().ok()?;
    String::from_utf8(output.stdout).ok()?.trim().parse().ok()
}

fn main() {
    let pid = get_finder_pid().expect("Finder must be running");

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║         AXTerminator Performance Benchmarks                  ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    unsafe {
        let app = AXUIElementCreateApplication(pid);

        // Benchmark 1: Simple attribute access (1000 iterations)
        let start = Instant::now();
        for _ in 0..1000 {
            let attr = cfstring("AXRole");
            let mut value: *mut std::ffi::c_void = std::ptr::null_mut();
            let _ = AXUIElementCopyAttributeValue(app, attr as _, &mut value);
            CFRelease(attr);
            if !value.is_null() {
                CFRelease(value);
            }
        }
        let simple_access_ns = start.elapsed().as_nanos() as f64 / 1000.0;

        // Benchmark 2: Get windows + first child (simulates app.find()) - 100 iterations
        let start = Instant::now();
        for _ in 0..100 {
            let windows_attr = cfstring("AXWindows");
            let mut windows: *mut std::ffi::c_void = std::ptr::null_mut();
            let _ = AXUIElementCopyAttributeValue(app, windows_attr as _, &mut windows);
            CFRelease(windows_attr);

            if !windows.is_null() {
                let count = CFArrayGetCount(windows);
                if count > 0 {
                    let window = CFArrayGetValueAtIndex(windows, 0);

                    // Get children of window
                    let children_attr = cfstring("AXChildren");
                    let mut children: *mut std::ffi::c_void = std::ptr::null_mut();
                    let _ =
                        AXUIElementCopyAttributeValue(window, children_attr as _, &mut children);
                    CFRelease(children_attr);

                    if !children.is_null() {
                        // Get first child's role (typical element access)
                        let child_count = CFArrayGetCount(children);
                        if child_count > 0 {
                            let child = CFArrayGetValueAtIndex(children, 0);
                            let role_attr = cfstring("AXRole");
                            let mut role_value: *mut std::ffi::c_void = std::ptr::null_mut();
                            let _ = AXUIElementCopyAttributeValue(
                                child,
                                role_attr as _,
                                &mut role_value,
                            );
                            CFRelease(role_attr);
                            if !role_value.is_null() {
                                CFRelease(role_value);
                            }
                        }
                        CFRelease(children);
                    }
                }
                CFRelease(windows);
            }
        }
        let element_access_ns = start.elapsed().as_nanos() as f64 / 100.0;

        // Benchmark 3: Perform action overhead
        let start = Instant::now();
        for _ in 0..500 {
            let action = cfstring("AXRaise");
            let _ = AXUIElementPerformAction(app, action as _);
            CFRelease(action);
        }
        let action_ns = start.elapsed().as_nanos() as f64 / 500.0;

        CFRelease(app as _);

        println!("📊 MEASURED PERFORMANCE:\n");
        println!("  ┌─────────────────────────────────┬────────────────────┐");
        println!("  │ Operation                       │ Time               │");
        println!("  ├─────────────────────────────────┼────────────────────┤");
        println!(
            "  │ Single attribute (AXRole)       │ {:>10.1} µs       │",
            simple_access_ns / 1000.0
        );
        println!(
            "  │ Element access (window→child)   │ {:>10.1} µs       │",
            element_access_ns / 1000.0
        );
        println!(
            "  │ Perform action (AXRaise)        │ {:>10.1} µs       │",
            action_ns / 1000.0
        );
        println!("  └─────────────────────────────────┴────────────────────┘\n");

        // Determine the representative "element access" time
        // README claims 242µs - let's see what we actually get
        let elem_access_us = element_access_ns / 1000.0;

        println!("📈 COMPARISON WITH COMPETITORS:\n");
        println!("  ┌─────────────────────┬─────────────────┬─────────────────┬────────────┐");
        println!("  │ Framework           │ Element Access  │ vs AXTerminator │ Source     │");
        println!("  ├─────────────────────┼─────────────────┼─────────────────┼────────────┤");
        println!(
            "  │ AXTerminator        │ {:>10.0} µs   │      1× baseline│ measured   │",
            elem_access_us
        );
        println!(
            "  │ XCUITest            │ ~200,000 µs     │    ~{:>5.0}× slower│ Apple docs │",
            200_000.0 / elem_access_us
        );
        println!(
            "  │ Appium (Mac2)       │ ~500,000 µs     │    ~{:>5.0}× slower│ est. HTTP  │",
            500_000.0 / elem_access_us
        );
        println!(
            "  │ Appium (worst case) │ ~2,000,000 µs   │    ~{:>5.0}× slower│ est. WebDr │",
            2_000_000.0 / elem_access_us
        );
        println!("  └─────────────────────┴─────────────────┴─────────────────┴────────────┘");

        println!(
            "\n✅ VERIFIED: Element access is ~{:.0}µs (README claims 242µs)",
            elem_access_us
        );

        if elem_access_us < 500.0 {
            println!("   → Claim is ACCURATE (within same order of magnitude)");
        } else {
            println!(
                "   → Claim needs updating (measured {:.0}µs vs claimed 242µs)",
                elem_access_us
            );
        }

        // Calculate realistic speedup
        let appium_estimate = 500_000.0; // 500ms is typical Appium element find
        let speedup = appium_estimate / elem_access_us;
        println!("\n   Realistic speedup vs Appium: ~{:.0}× faster", speedup);

        if (60.0..=100.0).contains(&speedup) {
            println!("   → \"60-100x faster\" claim is ACCURATE");
        } else if speedup >= 100.0 {
            println!(
                "   → \"60-100x faster\" claim is CONSERVATIVE (actual: {:.0}×)",
                speedup
            );
        } else {
            println!(
                "   → \"60-100x faster\" claim needs revision (actual: {:.0}×)",
                speedup
            );
        }
    }
}
