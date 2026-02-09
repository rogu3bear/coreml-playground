use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::toast::{ToastLevel, ToastStore};

// ---------------------------------------------------------------------------
// PersonaMode
// ---------------------------------------------------------------------------

/// UI persona controlling which features are surfaced.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PersonaMode {
    /// Simplified UI, plain-English descriptions.
    Explorer,
    /// Current UI as-is with introspection.
    Developer,
    /// Full UI + timing graphs, batch queue, raw JSON toggle.
    Researcher,
}

impl PersonaMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Explorer => "Explorer",
            Self::Developer => "Developer",
            Self::Researcher => "Researcher",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Explorer => "Simple chat interface, great for trying models",
            Self::Developer => "Full features with model introspection",
            Self::Researcher => "Advanced analysis with timing and batch tools",
        }
    }
}

impl Default for PersonaMode {
    fn default() -> Self {
        Self::Explorer
    }
}

// ---------------------------------------------------------------------------
// ComplexityTier
// ---------------------------------------------------------------------------

/// Progressive-unlock tier derived from usage counters.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub enum ComplexityTier {
    /// Chat only, no sidebar, no introspection, minimal model switcher.
    Tier1,
    /// Session sidebar, introspection, image drop zone.
    Tier2,
    /// All features unlocked.
    Tier3,
}

impl ComplexityTier {
    /// Derive the tier from usage counters.
    pub fn from_usage(session_count: u32, inference_count: u32) -> Self {
        if inference_count >= 20 {
            Self::Tier3
        } else if session_count >= 5 {
            Self::Tier2
        } else {
            Self::Tier1
        }
    }
}

impl Default for ComplexityTier {
    fn default() -> Self {
        Self::Tier1
    }
}

// ---------------------------------------------------------------------------
// Visibility helpers
// ---------------------------------------------------------------------------

/// Whether the session sidebar should be displayed.
pub fn should_show_sidebar(persona: PersonaMode, tier: ComplexityTier) -> bool {
    match persona {
        PersonaMode::Explorer => tier >= ComplexityTier::Tier2,
        PersonaMode::Developer | PersonaMode::Researcher => true,
    }
}

/// Whether the introspection panel should be available.
pub fn should_show_introspection(persona: PersonaMode, tier: ComplexityTier) -> bool {
    match persona {
        PersonaMode::Explorer => tier >= ComplexityTier::Tier2,
        PersonaMode::Developer | PersonaMode::Researcher => true,
    }
}

/// Whether batch-queue tools should be available.
pub fn should_show_batch(persona: PersonaMode) -> bool {
    matches!(persona, PersonaMode::Researcher)
}

/// Whether the raw-JSON toggle should be available.
pub fn should_show_raw_json(persona: PersonaMode) -> bool {
    matches!(persona, PersonaMode::Researcher)
}

// ---------------------------------------------------------------------------
// localStorage helpers (hydrate-only)
// ---------------------------------------------------------------------------

const PERSONA_KEY: &str = "persona_mode";
const SESSION_COUNT_KEY: &str = "session_count";
const INFERENCE_COUNT_KEY: &str = "inference_count";
const MANUAL_UNLOCK_KEY: &str = "manual_unlock_all";

/// Read a string from localStorage. Returns `None` on SSR or if the key is absent.
fn ls_get(key: &str) -> Option<String> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(key).ok().flatten())
        } else {
            let _ = key;
            None
        }
    }
}

/// Write a string to localStorage. No-op on SSR.
fn ls_set(key: &str, value: &str) {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
            {
                let _ = storage.set_item(key, value);
            }
        } else {
            let _ = (key, value);
        }
    }
}

fn load_persona() -> PersonaMode {
    ls_get(PERSONA_KEY)
        .and_then(|v| serde_json::from_str(&v).ok())
        .unwrap_or_default()
}

fn save_persona(mode: PersonaMode) {
    if let Ok(json) = serde_json::to_string(&mode) {
        ls_set(PERSONA_KEY, &json);
    }
}

fn load_counter(key: &str) -> u32 {
    ls_get(key)
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
}

fn save_counter(key: &str, value: u32) {
    ls_set(key, &value.to_string());
}

fn load_manual_unlock() -> bool {
    ls_get(MANUAL_UNLOCK_KEY)
        .map(|v| v == "true")
        .unwrap_or(false)
}

fn save_manual_unlock(value: bool) {
    ls_set(MANUAL_UNLOCK_KEY, if value { "true" } else { "false" });
}

// ---------------------------------------------------------------------------
// Context provider
// ---------------------------------------------------------------------------

/// Initialise persona + complexity signals and provide them via context.
///
/// Call this once near the root of the component tree.  Other components can
/// then `use_context::<ReadSignal<PersonaMode>>()` etc.
///
/// Returns the signal pairs so the caller can use them directly if needed.
pub fn provide_persona_context() -> (
    ReadSignal<PersonaMode>,
    WriteSignal<PersonaMode>,
    ReadSignal<ComplexityTier>,
    WriteSignal<ComplexityTier>,
) {
    let initial_persona = load_persona();
    let (persona_read, persona_write) = signal(initial_persona);

    let session_count = load_counter(SESSION_COUNT_KEY);
    let inference_count = load_counter(INFERENCE_COUNT_KEY);
    let manual = load_manual_unlock();

    let initial_tier = if manual {
        ComplexityTier::Tier3
    } else {
        ComplexityTier::from_usage(session_count, inference_count)
    };
    let (tier_read, tier_write) = signal(initial_tier);

    provide_context(persona_read);
    provide_context(persona_write);
    provide_context(tier_read);
    provide_context(tier_write);

    (persona_read, persona_write, tier_read, tier_write)
}

/// Increment the session counter and potentially upgrade the tier.
pub fn record_session() {
    let new_count = load_counter(SESSION_COUNT_KEY) + 1;
    save_counter(SESSION_COUNT_KEY, new_count);
    maybe_upgrade_tier(new_count, load_counter(INFERENCE_COUNT_KEY));
}

/// Increment the inference counter and potentially upgrade the tier.
pub fn record_inference() {
    let new_count = load_counter(INFERENCE_COUNT_KEY) + 1;
    save_counter(INFERENCE_COUNT_KEY, new_count);
    maybe_upgrade_tier(load_counter(SESSION_COUNT_KEY), new_count);
}

/// Recompute the tier from current counters and update the signal if changed.
fn maybe_upgrade_tier(session_count: u32, inference_count: u32) {
    if load_manual_unlock() {
        return; // already fully unlocked
    }
    let new_tier = ComplexityTier::from_usage(session_count, inference_count);
    if let Some(write) = use_context::<WriteSignal<ComplexityTier>>() {
        let current = use_context::<ReadSignal<ComplexityTier>>()
            .map(|r| r.get_untracked())
            .unwrap_or_default();
        if new_tier > current {
            write.set(new_tier);
            // Show a toast for the newly-unlocked tier
            let msg = match new_tier {
                ComplexityTier::Tier2 => "Session history is now available",
                ComplexityTier::Tier3 => "All features are now unlocked",
                ComplexityTier::Tier1 => return, // shouldn't happen on upgrade
            };
            ToastStore::push(msg.to_string(), ToastLevel::Info);
        }
    }
}

// ---------------------------------------------------------------------------
// PersonaSettings component
// ---------------------------------------------------------------------------

/// Settings panel with persona mode selector and progressive-unlock controls.
///
/// Renders a three-dot menu button that toggles the settings dropdown.
/// Place `<PersonaSettings/>` anywhere in the tree after `provide_persona_context()`.
#[component]
pub fn PersonaSettings() -> impl IntoView {
    let (open, set_open) = signal(false);

    let persona = use_context::<ReadSignal<PersonaMode>>()
        .expect("PersonaMode ReadSignal context");
    let set_persona = use_context::<WriteSignal<PersonaMode>>()
        .expect("PersonaMode WriteSignal context");
    let tier = use_context::<ReadSignal<ComplexityTier>>()
        .expect("ComplexityTier ReadSignal context");
    let set_tier = use_context::<WriteSignal<ComplexityTier>>()
        .expect("ComplexityTier WriteSignal context");

    let select_mode = move |mode: PersonaMode| {
        set_persona.set(mode);
        save_persona(mode);
    };

    let unlock_all = move |_| {
        save_manual_unlock(true);
        set_tier.set(ComplexityTier::Tier3);
        ToastStore::push(
            "All features are now unlocked".to_string(),
            ToastLevel::Info,
        );
    };

    // Close dropdown when clicking outside (hydrate-only)
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;

            let handler = Closure::<dyn Fn(web_sys::Event)>::new(move |ev: web_sys::Event| {
                if let Some(target) = ev.target() {
                    if let Some(el) = target.dyn_ref::<web_sys::HtmlElement>() {
                        // Walk up to see if we're inside the settings container
                        let inside = el.closest("[data-persona-settings]")
                            .ok()
                            .flatten()
                            .is_some();
                        if !inside {
                            set_open.set(false);
                        }
                    }
                }
            });
            if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                let _ = document.add_event_listener_with_callback(
                    "mousedown",
                    handler.as_ref().unchecked_ref(),
                );
            }
            handler.forget();
        }
    }

    let modes = [
        PersonaMode::Explorer,
        PersonaMode::Developer,
        PersonaMode::Researcher,
    ];

    view! {
        <div class="fixed top-3 right-10 z-50" data-persona-settings="">
            // Three-dot menu button
            <button
                class="p-1.5 rounded-lg text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors duration-150"
                on:click=move |_| set_open.update(|v| *v = !*v)
                title="Settings"
            >
                <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 6.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 12.75a.75.75 0 110-1.5.75.75 0 010 1.5zM12 18.75a.75.75 0 110-1.5.75.75 0 010 1.5z"/>
                </svg>
            </button>

            // Dropdown panel
            <Show when=move || open.get()>
                <div class="absolute right-0 top-full mt-1 w-72 rounded-xl border border-zinc-800 bg-zinc-900 shadow-xl">
                    // Header
                    <div class="px-4 pt-3 pb-2 border-b border-zinc-800">
                        <span class="text-xs font-medium uppercase tracking-wider text-zinc-500">"Persona Mode"</span>
                    </div>

                    // Mode options
                    <div class="p-2 space-y-1">
                        {modes.map(|mode| {
                            let is_active = move || persona.get() == mode;
                            view! {
                                <button
                                    class=move || {
                                        let base = "w-full text-left px-3 py-2 rounded-lg transition-colors duration-150 ";
                                        if is_active() {
                                            format!("{base}bg-amber-500/10 text-amber-400")
                                        } else {
                                            format!("{base}text-zinc-300 hover:bg-zinc-800")
                                        }
                                    }
                                    on:click=move |_| select_mode(mode)
                                >
                                    <div class="text-sm font-medium">{mode.label()}</div>
                                    <div class="text-xs text-zinc-500 mt-0.5">{mode.description()}</div>
                                </button>
                            }
                        }).collect_view()}
                    </div>

                    // Complexity tier info + unlock
                    <div class="px-4 py-3 border-t border-zinc-800">
                        <div class="flex items-center justify-between">
                            <span class="text-xs text-zinc-500">
                                {move || match tier.get() {
                                    ComplexityTier::Tier1 => "Tier 1 — Basic",
                                    ComplexityTier::Tier2 => "Tier 2 — Intermediate",
                                    ComplexityTier::Tier3 => "Tier 3 — Full Access",
                                }}
                            </span>
                            <Show when=move || tier.get() != ComplexityTier::Tier3>
                                <button
                                    class="text-xs text-amber-500 hover:text-amber-400 transition-colors"
                                    on:click=unlock_all
                                >
                                    "Unlock all features"
                                </button>
                            </Show>
                        </div>
                        // Progress dots
                        <div class="flex gap-1.5 mt-2">
                            <div class="w-2 h-2 rounded-full bg-amber-500"/>
                            <div class=move || {
                                if tier.get() >= ComplexityTier::Tier2 {
                                    "w-2 h-2 rounded-full bg-amber-500"
                                } else {
                                    "w-2 h-2 rounded-full bg-zinc-700"
                                }
                            }/>
                            <div class=move || {
                                if tier.get() >= ComplexityTier::Tier3 {
                                    "w-2 h-2 rounded-full bg-amber-500"
                                } else {
                                    "w-2 h-2 rounded-full bg-zinc-700"
                                }
                            }/>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}
