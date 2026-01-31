//! Benchmarks for element access performance
//!
//! Tests actual element access times via macOS Accessibility API
//!
//! Run with: cargo bench

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::process::Command;
use std::time::Duration;

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::CFString;

// FFI declarations for macOS Accessibility API
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: core_foundation::string::CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(
        element: CFTypeRef,
        action: core_foundation::string::CFStringRef,
    ) -> i32;
}

/// Get Finder PID (always running on macOS)
fn get_finder_pid() -> Option<i32> {
    let output = Command::new("pgrep").args(["-x", "Finder"]).output().ok()?;
    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Benchmark: Create AXUIElementRef for application
fn benchmark_create_app_element(c: &mut Criterion) {
    let pid = get_finder_pid().expect("Finder must be running");

    c.bench_function("create_app_element", |b| {
        b.iter(|| unsafe {
            let app_ref = AXUIElementCreateApplication(black_box(pid));
            CFRelease(app_ref);
            black_box(app_ref)
        })
    });
}

/// Benchmark: Get single attribute (AXRole)
fn benchmark_get_attribute(c: &mut Criterion) {
    let pid = get_finder_pid().expect("Finder must be running");

    unsafe {
        let app_ref = AXUIElementCreateApplication(pid);

        c.bench_function("get_ax_role", |b| {
            b.iter(|| {
                let mut value: CFTypeRef = std::ptr::null();
                let attr = CFString::new("AXRole");
                let result = AXUIElementCopyAttributeValue(
                    black_box(app_ref),
                    attr.as_concrete_TypeRef(),
                    &mut value,
                );
                if result == 0 && !value.is_null() {
                    CFRelease(value);
                }
                black_box(result)
            })
        });

        CFRelease(app_ref);
    }
}

/// Benchmark: Get children array (typically largest operation)
fn benchmark_get_children(c: &mut Criterion) {
    let pid = get_finder_pid().expect("Finder must be running");

    unsafe {
        let app_ref = AXUIElementCreateApplication(pid);

        // First get windows
        let mut windows: CFTypeRef = std::ptr::null();
        let windows_attr = CFString::new("AXWindows");
        let result =
            AXUIElementCopyAttributeValue(app_ref, windows_attr.as_concrete_TypeRef(), &mut windows);

        if result == 0 && !windows.is_null() {
            use core_foundation::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};

            let array = windows as CFArrayRef;
            let count = CFArrayGetCount(array);

            if count > 0 {
                let window = CFArrayGetValueAtIndex(array, 0);

                c.bench_function("get_children", |b| {
                    b.iter(|| {
                        let mut children: CFTypeRef = std::ptr::null();
                        let children_attr = CFString::new("AXChildren");
                        let result = AXUIElementCopyAttributeValue(
                            black_box(window),
                            children_attr.as_concrete_TypeRef(),
                            &mut children,
                        );
                        if result == 0 && !children.is_null() {
                            CFRelease(children);
                        }
                        black_box(result)
                    })
                });
            }

            CFRelease(windows);
        }

        CFRelease(app_ref);
    }
}

/// Benchmark: Full element search (breadth-first through tree)
fn benchmark_element_search(c: &mut Criterion) {
    let pid = get_finder_pid().expect("Finder must be running");

    unsafe {
        let app_ref = AXUIElementCreateApplication(pid);

        c.bench_function("search_first_button", |b| {
            b.iter(|| {
                let found = search_for_role(black_box(app_ref), "AXButton", 15);
                black_box(found)
            })
        });

        CFRelease(app_ref);
    }
}

/// Search for element with given role (BFS, limited depth)
unsafe fn search_for_role(element: CFTypeRef, target_role: &str, max_depth: usize) -> bool {
    use core_foundation::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};

    if max_depth == 0 || element.is_null() {
        return false;
    }

    // Check this element's role
    let mut role_value: CFTypeRef = std::ptr::null();
    let role_attr = CFString::new("AXRole");
    if AXUIElementCopyAttributeValue(element, role_attr.as_concrete_TypeRef(), &mut role_value) == 0
        && !role_value.is_null()
    {
        let role_cf = CFString::wrap_under_get_rule(role_value as _);
        let matches = role_cf.to_string() == target_role;
        CFRelease(role_value);
        if matches {
            return true;
        }
    }

    // Get children
    let mut children: CFTypeRef = std::ptr::null();
    let children_attr = CFString::new("AXChildren");
    if AXUIElementCopyAttributeValue(element, children_attr.as_concrete_TypeRef(), &mut children)
        == 0
        && !children.is_null()
    {
        let array = children as CFArrayRef;
        let count = CFArrayGetCount(array);

        for i in 0..count.min(50) {
            // Limit to avoid huge trees
            let child = CFArrayGetValueAtIndex(array, i);
            if search_for_role(child, target_role, max_depth - 1) {
                CFRelease(children);
                return true;
            }
        }

        CFRelease(children);
    }

    false
}

/// Benchmark: Perform action (background click)
fn benchmark_perform_action(c: &mut Criterion) {
    let pid = get_finder_pid().expect("Finder must be running");

    unsafe {
        let app_ref = AXUIElementCreateApplication(pid);

        // Measure AXUIElementPerformAction call overhead (will fail on app element, but measures API overhead)
        c.bench_function("perform_action_overhead", |b| {
            b.iter(|| {
                let action = CFString::new("AXRaise");
                let result =
                    AXUIElementPerformAction(black_box(app_ref), action.as_concrete_TypeRef());
                black_box(result)
            })
        });

        CFRelease(app_ref);
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(Duration::from_secs(5));
    targets =
        benchmark_create_app_element,
        benchmark_get_attribute,
        benchmark_get_children,
        benchmark_element_search,
        benchmark_perform_action
}
criterion_main!(benches);
