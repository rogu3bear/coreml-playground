/// Server function stubs are always visible so Leptos can register them on
/// both the client (WASM) and server (SSR) sides.
pub mod api;

/// Response interpretation is pure serde logic with no platform-specific
/// dependencies, so it compiles under both SSR and WASM targets.
pub mod interpreter;

/// The following modules contain server-only implementations and depend on
/// crates (axum, rusqlite, tokio, notify) that are only available under the
/// `ssr` feature flag.
#[cfg(feature = "ssr")]
pub mod inference;
#[cfg(feature = "ssr")]
pub mod model_registry;
#[cfg(feature = "ssr")]
pub mod middleware;
#[cfg(feature = "ssr")]
pub mod session_store;
