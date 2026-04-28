# AGENTS.md

CoreML Studio is a Rust SSR/WASM app with a Swift CoreML bridge. Keep the docs aligned with that split, the checked-in `Makefile`, and the current tests.

## Active Surfaces

- `src/` is the Rust app/runtime surface.
- `src/server/` owns server functions, inference orchestration, middleware, and persistence.
- `build.rs`, `src/bridge/ffi.rs`, and `swift/CoreMLBridge.swift` are the real CoreML bridge contract.
- `tests/` is the integration-test surface for sessions, inference, interpreter round-trips, middleware, model registry, and settings.
- `public/` and `style/` are the web shell assets.

## Canonical Commands

- Setup: `make setup`
- Mock dev: `make dev`
- Real bridge dev: `make dev-real`
- Check: `make check`
- Test: `make test`
- Lint: `make lint`
- Format: `make fmt`
- Build: `make build` or `make build-release`

## Guardrails

- `COREML_MOCK=1` is the default verification lane; only use the real Swift bridge when the task actually depends on it.
- Keep SSR and hydrate guidance grounded in the current feature gates and build flow.
- Keep bridge claims aligned with `build.rs`, `src/bridge/ffi.rs`, `swift/CoreMLBridge.swift`, and the server inference path.
- Treat `Makefile`, `Cargo.toml`, and the checked-in tests as the authority, not scratch notes or one-off experiments.
