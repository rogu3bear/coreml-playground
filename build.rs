// build.rs
// Compiles swift/CoreMLBridge.swift into a static object and links it with the
// CoreML and Foundation frameworks. Only runs on macOS for SSR builds.

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Print a cargo:warning and skip Swift compilation, falling back to mock mode.
macro_rules! warn_and_skip {
    ($($arg:tt)*) => {{
        println!("cargo:warning={}", format!($($arg)*));
        println!("cargo:warning=Swift bridge will not be available. App will run in mock mode.");
        return;
    }};
}

fn main() {
    // Rerun if the COREML_MOCK env var changes.
    println!("cargo:rerun-if-env-changed=COREML_MOCK");

    // Only compile the Swift bridge on macOS. WASM and other targets skip entirely.
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os != "macos" || target_arch == "wasm32" {
        return;
    }

    // Allow explicitly skipping Swift compilation via COREML_MOCK=1.
    if env::var("COREML_MOCK").unwrap_or_default() == "1" {
        println!("cargo:warning=COREML_MOCK=1 set — skipping Swift bridge compilation.");
        println!("cargo:warning=Swift bridge will not be available. App will run in mock mode.");
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"));
    let swift_source = manifest_dir.join("swift").join("CoreMLBridge.swift");

    // Ensure the Swift source file exists.
    if !swift_source.exists() {
        warn_and_skip!(
            "Swift source not found at {}. Cannot build CoreML bridge.",
            swift_source.display()
        );
    }

    // Rerun if the Swift file changes.
    println!("cargo:rerun-if-changed={}", swift_source.display());

    // Determine the macOS SDK path.
    let sdk_path = match Command::new("xcrun")
        .args(["--show-sdk-path", "--sdk", "macosx"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            warn_and_skip!("Failed to run xcrun --show-sdk-path: {e}. Is Xcode installed?");
        }
    };

    if !sdk_path.status.success() {
        warn_and_skip!(
            "xcrun failed: {}",
            String::from_utf8_lossy(&sdk_path.stderr)
        );
    }
    let sdk = String::from_utf8(sdk_path.stdout)
        .expect("Invalid UTF-8 from xcrun")
        .trim()
        .to_string();

    // Determine architecture-specific target triple.
    let target_triple = match target_arch.as_str() {
        "aarch64" => "arm64-apple-macosx14.0",
        "x86_64" => "x86_64-apple-macosx14.0",
        other => {
            warn_and_skip!("Unsupported macOS architecture for Swift bridge: {other}");
        }
    };

    // Find the Swift runtime library path for linking.
    let swift_lib_output = match Command::new("xcrun")
        .args(["--toolchain", "default", "-f", "swiftc"])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            warn_and_skip!("Failed to locate swiftc via xcrun: {e}. Is Xcode installed?");
        }
    };

    let swiftc_path = String::from_utf8(swift_lib_output.stdout)
        .expect("Invalid UTF-8 from swiftc path")
        .trim()
        .to_string();

    // Derive the Swift lib directory from the swiftc binary path.
    // Typical: /usr/bin/swiftc -> /usr/lib/swift
    // Xcode:   .../Toolchains/.../usr/bin/swiftc -> .../usr/lib/swift/macosx
    let swiftc_dir = PathBuf::from(&swiftc_path);
    let toolchain_lib = swiftc_dir
        .parent() // bin/
        .and_then(|p| p.parent()) // usr/
        .map(|p| p.join("lib").join("swift").join("macosx"));

    // Determine optimization level.
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let opt_flag = if profile == "release" { "-O" } else { "-Onone" };

    // Compile Swift to an object file.
    let object_path = out_dir.join("CoreMLBridge.o");

    let status = match Command::new("swiftc")
        .args([
            "-emit-object",
            "-static",
            "-parse-as-library",
            "-sdk",
            &sdk,
            "-target",
            target_triple,
            opt_flag,
            "-o",
            object_path.to_str().unwrap(),
            swift_source.to_str().unwrap(),
        ])
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            warn_and_skip!("Failed to invoke swiftc: {e}. Is Swift installed?");
        }
    };

    if !status.success() {
        warn_and_skip!("swiftc compilation failed. Check swift/CoreMLBridge.swift for errors.");
    }

    // Create a static library from the object file.
    let lib_path = out_dir.join("libcoreml_bridge.a");
    let ar_status = match Command::new("ar")
        .args(["rcs", lib_path.to_str().unwrap(), object_path.to_str().unwrap()])
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            warn_and_skip!("Failed to invoke ar: {e}");
        }
    };

    if !ar_status.success() {
        warn_and_skip!("Failed to create static library from Swift object file.");
    }

    // Emit linker directives.
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=coreml_bridge");

    // Link macOS frameworks.
    println!("cargo:rustc-link-lib=framework=CoreML");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=CoreVideo");
    println!("cargo:rustc-link-lib=framework=CoreGraphics");
    println!("cargo:rustc-link-lib=framework=AppKit");

    // Link Swift standard libraries.
    if let Some(lib_dir) = toolchain_lib {
        if lib_dir.exists() {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
        }
    }

    // Also check the platform-specific lib directory inside the SDK.
    let sdk_swift_lib = PathBuf::from(&sdk)
        .join("usr")
        .join("lib")
        .join("swift");
    if sdk_swift_lib.exists() {
        println!("cargo:rustc-link-search=native={}", sdk_swift_lib.display());
    }

    // Xcode 15+ often stores Swift dylibs here.
    let xcode_swift_lib = PathBuf::from("/usr/lib/swift");
    if xcode_swift_lib.exists() {
        println!(
            "cargo:rustc-link-search=native={}",
            xcode_swift_lib.display()
        );
    }
}
