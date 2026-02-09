// CoreML bridge — Swift FFI bindings
// This module provides the Rust interface to CoreML via Swift interop.
//
// Architecture:
//   Rust (this module) <-> C FFI <-> Swift (swift/CoreMLBridge.swift) <-> CoreML framework
//
// The Swift code is compiled by build.rs and linked as a static library.
// All FFI functions are gated behind #[cfg(feature = "ssr")] since CoreML
// only runs on the server (macOS), never in WASM.

#[cfg(feature = "ssr")]
mod ffi;

#[cfg(feature = "ssr")]
pub use ffi::*;

// Stub for WASM builds — components can reference bridge types without cfg gates
#[cfg(not(feature = "ssr"))]
pub fn available() -> bool {
    false
}

#[cfg(feature = "ssr")]
pub fn available() -> bool {
    true
}
