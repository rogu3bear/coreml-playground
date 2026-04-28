# CLAUDE.md

CoreML Studio (`coreml-playground`) is a Leptos 0.8 + Axum app that runs Apple CoreML models locally through a Swift bridge. Keep the Rust SSR/WASM split and the Swift bridge contract explicit.

## Core Commands

```sh
make setup
make dev
make dev-real
make check
make test
make lint
make fmt
make build
make build-release
```

Run a targeted test with `COREML_MOCK=1 cargo test --features ssr test_name`.

## Repo Shape

- `src/` is the Rust app/runtime surface.
- `src/server/` owns inference orchestration, middleware, model registry, and persistence.
- `build.rs`, `src/bridge/ffi.rs`, and `swift/CoreMLBridge.swift` define the real CoreML bridge contract.
- `tests/` covers inference, interpreter round-trips, middleware, model registry, session persistence, settings, and shared type contracts.
- `public/` and `style/` are the web shell assets.

## Working Rules

- Default to `COREML_MOCK=1` unless the task specifically needs the real Swift/CoreML path.
- Keep server-only code behind `ssr` gates and browser-only code behind `hydrate` gates.
- Keep bridge claims aligned with `build.rs`, `src/bridge/ffi.rs`, and `swift/CoreMLBridge.swift`.
- Keep runtime behavior grounded in the checked-in tests and `Makefile` instead of ad hoc notes.
- Use the toast/error patterns already in the UI; do not add modal alert-style error handling.
