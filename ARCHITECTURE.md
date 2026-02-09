# Architecture

CoreML Studio is a Leptos 0.8 + Axum web application that runs CoreML models
locally on macOS. The browser talks to a server-side-rendered Leptos app backed
by an Axum HTTP server which delegates inference to a Swift bridge over FFI.

## High-Level System Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│                          Browser                                 │
│  ┌────────────┐  ┌────────────┐  ┌─────────────┐               │
│  │ Leptos WASM│  │  Hydration │  │  WebSocket   │               │
│  │ Components │◄─┤  Scripts   │  │  (streaming) │               │
│  └─────┬──────┘  └────────────┘  └──────┬───────┘               │
│        │  server fns (RPC)               │ tokens                │
└────────┼─────────────────────────────────┼───────────────────────┘
         │                                 │
─ ─ ─ ─ ┼ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┼ ─ ─ ─ HTTP / WS ─ ─
         │                                 │
┌────────▼─────────────────────────────────▼───────────────────────┐
│                     Axum Server (SSR)                             │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  Leptos SSR Router                                       │    │
│  │  ┌──────────┐  ┌──────────────┐  ┌───────────────────┐  │    │
│  │  │ api.rs   │  │ session_store│  │ model_registry.rs │  │    │
│  │  │ (server  │  │ .rs (SQLite) │  │ (fs watcher +     │  │    │
│  │  │  fns)    │  │              │  │  metadata cache)  │  │    │
│  │  └────┬─────┘  └──────────────┘  └────────┬──────────┘  │    │
│  │       │                                    │             │    │
│  │  ┌────▼────────────────────────────────────▼──────────┐  │    │
│  │  │  inference.rs                                      │  │    │
│  │  │  (dispatch to CoreML bridge or mock)               │  │    │
│  │  └────────────────────┬───────────────────────────────┘  │    │
│  │                       │ FFI (C ABI)                      │    │
│  └───────────────────────┼──────────────────────────────────┘    │
│                          │                                       │
│  ┌───────────────────────▼───────────────────────────────────┐   │
│  │  CoreML Bridge (swift/CoreMLBridge.swift)                 │   │
│  │  Compiled to libcoreml_bridge.a via build.rs              │   │
│  │  Links: CoreML, Foundation, CoreVideo, CoreGraphics       │   │
│  └───────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

## Module Overview

### `src/app.rs` -- Root Component

- Defines the outer HTML shell (`shell()`) and Leptos `<Router>`.
- Creates global reactive signals: active model, session, theme, keyboard
  shortcuts, introspection toggle, model picker state.
- Provides all signals as Leptos context so any descendant can consume them.
- Registers client-side keyboard shortcuts (Cmd+K model picker, Cmd+N new
  session, Cmd+I introspection, Escape dismiss).

### `src/types.rs` -- Shared Types

Domain types compiled under both `ssr` and `hydrate` features:

| Type               | Purpose                                       |
|--------------------|-----------------------------------------------|
| `ModelInfo`        | Model metadata (id, name, type, schema, size) |
| `ModelType`        | Enum: Text, Vision, Multimodal, Audio, Unknown|
| `PortInfo`         | Input/output port description with shape       |
| `ChatMessage`      | Single message in a session                    |
| `MessageContent`   | Text, Image, ModelOutput, Streaming, Batch     |
| `Session`          | Session header (id, model, timestamps, preview)|
| `InferenceRequest` | Client-to-server inference payload             |
| `InferenceInput`   | Text, Image, or BatchImages variant            |
| `InferenceResponse`| Server-to-client result with latency           |
| `WsMessage`        | WebSocket frame: Token, Done, or Error         |

### `src/components/` -- UI Components

| Module              | Description                                          |
|---------------------|------------------------------------------------------|
| `chat`              | Chat timeline, message bubbles, scroll management    |
| `input_bar`         | Adaptive text/image input area with drag-drop        |
| `model_switcher`    | Model lens bar and picker overlay (Cmd+K)            |
| `session_sidebar`   | Session list, new/delete/switch sessions             |
| `introspection`     | Side panel showing model internals (Cmd+I)           |
| `comparison`        | Side-by-side model comparison view                   |
| `command_palette`   | Keyboard-driven command palette                      |
| `visualization`     | Output visualization (charts, heatmaps)              |
| `toast`             | Ephemeral notification system                        |
| `resizable`         | Drag-to-resize panel splitter                        |
| `onboarding`        | First-run multi-step onboarding + landing hero       |
| `settings`          | User preferences panel                               |
| `model_diff`        | Diff two models' schemas                             |
| `export`            | Export sessions / results                            |

### `src/server/` -- Backend

| Module              | Feature Gate | Description                                |
|---------------------|--------------|--------------------------------------------|
| `api.rs`            | always       | Leptos `#[server]` functions (RPC stubs registered on both client and server) |
| `interpreter.rs`    | always       | Pure serde logic for parsing model output  |
| `inference.rs`      | `ssr`        | Dispatches inference to CoreML bridge or mock backend |
| `model_registry.rs` | `ssr`        | Watches `~/CoreML-Models/`, caches model metadata |
| `session_store.rs`  | `ssr`        | SQLite-backed session and message persistence |
| `middleware.rs`     | `ssr`        | Axum middleware (CORS, static files, etc.)  |

### `build.rs` -- CoreML Swift Bridge Compilation

Compiles `swift/CoreMLBridge.swift` into a static library
(`libcoreml_bridge.a`) using `swiftc`, then links it with the CoreML,
Foundation, CoreVideo, CoreGraphics, and AppKit frameworks. Only runs on
macOS for native architecture builds. When `COREML_MOCK=1` is set or when
targeting WASM, the bridge is skipped and the app runs in mock mode.

### `style/main.css` -- Tailwind + Design Tokens

Tailwind CSS with a `@layer base` block defining semantic design tokens
(colours, spacing, radii, transition durations) as CSS custom properties.
Component-specific classes live in `@layer components`. Animations for the
streaming cursor, model load, lens refocus, and skeleton loading are defined
as `@keyframes` at the end of the file.

## Feature Flags

| Flag       | Purpose                                               |
|------------|-------------------------------------------------------|
| `ssr`      | Server-side rendering. Enables Axum, SQLite, tokio, filesystem watcher, CoreML bridge linking. |
| `hydrate`  | Client-side hydration. Enables WASM bindings, web-sys, gloo-timers, console panic hook. |

The two features are mutually exclusive at runtime: `ssr` compiles the server
binary, `hydrate` compiles the WASM client library.

## Build & Run

### Prerequisites

- Rust (stable, 2021 edition)
- `wasm32-unknown-unknown` target: `rustup target add wasm32-unknown-unknown`
- `cargo-leptos`: `cargo install cargo-leptos`
- macOS with Xcode (for the CoreML Swift bridge; set `COREML_MOCK=1` to skip)

### Development

```sh
cargo leptos watch          # starts dev server on 127.0.0.1:3100
```

This concurrently compiles the SSR server binary and the WASM client,
watches for file changes, and hot-reloads.

### Production

```sh
cargo leptos build --release
```

The output lands in `target/site/` with the server binary at
`target/release/coreml-playground`.

### Testing

```sh
cargo test --features ssr   # run server-side unit and integration tests
```

### Mock Mode

If you do not have a Mac or do not want to install Xcode:

```sh
COREML_MOCK=1 cargo leptos watch
```

The app will start without the Swift bridge and return mock inference results.
