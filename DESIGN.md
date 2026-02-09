# Design Philosophy

CoreML Studio is a browser-based tool for exploring, testing, and comparing Apple
CoreML models. Every design decision serves one goal: **make the model the
protagonist, not the UI**.

## Branding

The display name is **CoreML Studio** -- conveying a professional creative
workspace rather than a sandbox. The package/crate remains `coreml-playground`
in code and Cargo.toml to avoid a disruptive rename across the build toolchain.

## Core Principles

### The Lens Metaphor

Switching models should feel like changing a camera lens, not switching apps.
The model switcher (internally called `ModelLens`) has no visible chrome: no
dropdown arrows, no outlines at rest. Hover states produce a sense of *depth*
rather than highlight -- a subtle background shift that implies you are looking
*through* the lens, not clicking a button. When a new model loads the view
"refocuses" with a blur-to-sharp transition under 300 ms.

### Chat-First Interaction

The most natural way to explore a model is to talk to it. Every model type
(text, vision, multimodal, audio) funnels through the same chat interface.
Images are dragged or pasted, prompts are typed, results stream back inline.
The chat timeline is the single source of truth for every interaction.

### Progressive Disclosure

First-time users see an onboarding flow with three steps and a landing hero.
From there the interface is deliberately minimal: a sidebar, a chat area, and
the model lens. Advanced capabilities (introspection panel, comparison mode,
command palette, model diff, export, visualization) reveal themselves through
keyboard shortcuts or contextual hints -- never by cluttering the default view.

### Amber on Zinc

The colour palette is zinc greys (zinc-950 through zinc-100) with amber as the
sole accent colour. Amber brings warmth to a technical tool without competing
for attention. The accent appears in the streaming cursor, active step
indicators, CTA buttons, and the sweep animation on model load. Design tokens
in `style/main.css` formalise this as `--accent` / `--accent-hover` /
`--accent-muted`.

### No Visible Chrome

Borders are deliberately low-contrast (`zinc-800/50`). Panels separate via
spacing and background tint, not hard lines. Buttons at rest are nearly
invisible, gaining presence only on hover. The theme toggle sits fixed at the
top-right corner with a small icon and no label.

### Transitions Under 200 ms

Perceived responsiveness matters more than raw latency. The design-token
`--transition-fast` is 100 ms, `--transition-normal` 200 ms. Hover feedback,
panel reveals, and toast entrances all resolve within these budgets. The lens
refocus animation is the only deliberate exception at 300 ms because it
communicates a meaningful state change.

### Performance Matters

- **Streaming responses**: model output streams token-by-token via WebSocket,
  rendered with a breathing cursor that gives tactile feedback on arrival.
- **Skeleton loading**: placeholder UI renders immediately while data loads.
- **Lazy initialization**: the Swift-to-Rust CoreML bridge is compiled at build
  time but models are loaded on first access, not at startup.
- **WASM size**: the `wasm-release` profile uses `opt-level = 'z'`, LTO, and
  single codegen unit to minimise the hydration bundle.

## Visual Language

### Palette

- **Background**: zinc-950 (`#09090b`)
- **Cards / Surfaces**: zinc-900 / zinc-800 with subtle borders (`zinc-800/50`)
- **Text**: zinc-100 (primary), zinc-400 (secondary), zinc-500 (tertiary)
- **Accent**: amber-500 (`#f59e0b`) -- used sparingly for CTAs, active states,
  progress indicators
- **Error**: red-500 -- inline, not modal
- **Font**: system sans-serif (Inter preferred); monospace for code and schemas

### Interaction States

| Element           | Resting State           | Active / Hover          |
|-------------------|-------------------------|-------------------------|
| Model lens        | Transparent background  | `zinc-900/50` tint      |
| Sidebar session   | No border               | `zinc-800/50` bg        |
| Active session    | `zinc-800/70` bg        | `zinc-700/50` border    |
| CTA button        | Amber-500 solid         | Amber-400 solid         |
| Theme toggle      | `zinc-500` icon         | `zinc-300` icon         |
| Toast             | Slides in from bottom   | Fades after timeout     |
| Streaming cursor  | Breathe animation       | Receive pulse on token  |

### Loading States

- Never use a generic spinner.
- Show what is happening: "Loading model...", "Running inference..."
- Use skeleton states that match the shape of what is loading.
- Minimum 200 ms display time to prevent flash.

### Error States

- Inline, not modal.
- Red-tinted variant of the normal element.
- Always include a recovery action (retry, dismiss, alternative).
- Errors persist until dismissed; warnings auto-dismiss in 5 s.

### Design Principles for New Elements

- Progressive disclosure over settings pages.
- Inline feedback over modal dialogs.
- Contextual controls over persistent toolbars.
- Animation that communicates state, not decoration.
