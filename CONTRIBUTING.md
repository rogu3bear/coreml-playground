# Contributing

Thanks for your interest in CoreML Studio. This guide covers everything you
need to get a development environment running and submit changes.

## Quick Start

```sh
# 1. Clone
git clone <repo-url> && cd coreml-playground

# 2. Install the WASM target
rustup target add wasm32-unknown-unknown

# 3. Install cargo-leptos
cargo install cargo-leptos

# 4. Run in dev mode (mock inference if no Xcode)
COREML_MOCK=1 cargo leptos watch

# 5. Open http://127.0.0.1:3100
```

## Prerequisites

| Tool            | Version     | Notes                                      |
|-----------------|-------------|--------------------------------------------|
| Rust            | stable      | 2021 edition                               |
| wasm32 target   | --          | `rustup target add wasm32-unknown-unknown` |
| cargo-leptos    | latest      | `cargo install cargo-leptos`               |
| Xcode / swiftc  | 15+         | Only needed for real CoreML inference       |
| Node (optional) | 18+         | Only if modifying Tailwind config          |

On non-macOS systems, or without Xcode, set `COREML_MOCK=1` to skip the Swift
bridge and run with mock model responses.

## Project Layout

```
src/
  app.rs              Root component, router, global signals
  types.rs            Shared domain types (both SSR and WASM)
  components/         UI components (Leptos 0.8)
    chat.rs           Chat timeline and message bubbles
    input_bar.rs      Adaptive input area with drag-drop
    model_switcher.rs Model lens and picker overlay
    session_sidebar.rs Session management sidebar
    introspection.rs  Model internals panel
    comparison.rs     Side-by-side model comparison
    command_palette.rs Keyboard command palette
    visualization.rs  Output visualization
    toast.rs          Notification system
    resizable.rs      Drag-to-resize splitter
    onboarding.rs     First-run onboarding + landing hero
    settings.rs       User preferences
    model_diff.rs     Model schema diff
    export.rs         Session/result export
  server/             Server modules (Axum, SQLite, inference)
    api.rs            Leptos server functions
    interpreter.rs    Response parsing (pure serde)
    inference.rs      CoreML bridge dispatch (ssr only)
    model_registry.rs Model discovery with fs watcher (ssr only)
    session_store.rs  SQLite persistence (ssr only)
    middleware.rs     Axum middleware (ssr only)
swift/
  CoreMLBridge.swift  CoreML FFI bridge (compiled by build.rs)
style/
  main.css            Tailwind + design tokens
public/               Static assets
build.rs              Swift bridge build script
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for a detailed module map and system
diagram.

## Code Style

### Rust

- **Edition**: 2021
- **Formatting**: `cargo fmt` before committing
- **Linting**: `cargo clippy --features ssr` should produce no warnings
- **Imports**: group std, external crates, then crate-internal; alphabetise
  within groups

### Leptos 0.8 Patterns

- **Feature gating**: use `cfg_if::cfg_if!` to separate `hydrate`-only and
  `ssr`-only code within components. Never import `web_sys` outside a
  `#[cfg(feature = "hydrate")]` block.
- **Context signals**: global state is provided via `provide_context` in
  `app.rs` and consumed with `use_context` in child components. Do not create
  new global signals without updating `app.rs`.
- **Toast notifications**: use the toast component (`components::toast`) for
  user-facing messages. Do not use `window().alert()`.
- **Server functions**: define with `#[server(Name)]` in `server/api.rs`.
  Leptos auto-generates client stubs. Keep SSR-only imports behind
  `#[cfg(feature = "ssr")]`.

### CSS

- Prefer Tailwind utility classes directly in component `view!` macros.
- Reusable patterns go in `@layer components` in `style/main.css`.
- Use design tokens (`--accent`, `--bg-primary`, etc.) for themed values.
- Transitions must respect `--transition-fast` (100 ms) and
  `--transition-normal` (200 ms) budgets.

## Component Patterns

When adding a new component:

1. Create `src/components/my_component.rs`.
2. Add `pub mod my_component;` to `src/components/mod.rs`.
3. Use `#[component]` and return `impl IntoView`.
4. Gate any browser API usage:
   ```rust
   cfg_if::cfg_if! {
       if #[cfg(feature = "hydrate")] {
           // web_sys / wasm_bindgen code here
       }
   }
   ```
5. Accept props via function parameters; use `ReadSignal` / `WriteSignal` for
   reactive data.
6. Consume global state with `use_context::<ReadSignal<T>>()` (or
   `WriteSignal<T>`).

## Testing

```sh
cargo test --features ssr       # server-side tests
cargo clippy --features ssr     # lint check
cargo fmt -- --check            # formatting check
```

Write tests alongside the module they cover (`#[cfg(test)] mod tests { ... }`).
Integration tests that need SQLite should use `tempfile` for ephemeral
databases.

## Pull Request Process

1. **Branch** off `main` with a descriptive name (e.g. `feat/batch-export`).
2. **Keep PRs focused**: one feature or fix per PR.
3. **Run checks locally** before pushing:
   ```sh
   cargo fmt -- --check && cargo clippy --features ssr && cargo test --features ssr
   ```
4. **Write a clear description**: what changed, why, and how to test it.
5. **Screenshots or recordings** are welcome for UI changes.
6. PRs require at least one approving review before merge.

## Design Guidelines

See [DESIGN.md](DESIGN.md) for the full design philosophy. Key points:

- Amber accent on zinc palette only; do not introduce new accent colours.
- Transitions under 200 ms (300 ms only for meaningful state changes).
- Progressive disclosure: defaults should be simple; power features are
  accessible but not prominent.
- Performance-first: stream responses, use skeleton loading, lazy-init models.
