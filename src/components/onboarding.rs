use leptos::prelude::*;

// ---------------------------------------------------------------------------
// localStorage helpers (hydrate-only)
// ---------------------------------------------------------------------------

#[cfg(feature = "hydrate")]
const ONBOARDING_KEY: &str = "onboarding_complete";

#[cfg(feature = "hydrate")]
fn is_onboarding_complete() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(ONBOARDING_KEY).ok().flatten())
        .map(|v| v == "true")
        .unwrap_or(false)
}

fn mark_onboarding_complete() {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
            {
                let _ = storage.set_item(ONBOARDING_KEY, "true");
            }
        }
    }
}

/// Clears the onboarding flag so the flow will show again on next load.
/// Useful for testing / development.
pub fn reset_onboarding() {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
            {
                let _ = storage.remove_item(ONBOARDING_KEY);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Step indicator dots
// ---------------------------------------------------------------------------

#[component]
fn StepDots(current: ReadSignal<usize>, total: usize) -> impl IntoView {
    let dots = (0..total).collect::<Vec<_>>();
    view! {
        <div class="flex items-center gap-1.5">
            {dots.into_iter().map(|i| {
                let active = move || current.get() == i;
                view! {
                    <span
                        class="block h-2 rounded-full transition-all duration-200"
                        class:w-2=move || !active()
                        class:bg-zinc-600=move || !active()
                        class:w-6=active
                        class:bg-amber-500=active
                    />
                }
            }).collect_view()}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Step content
// ---------------------------------------------------------------------------

#[component]
fn StepWelcome() -> impl IntoView {
    view! {
        <div class="flex flex-col items-center text-center space-y-5">
            // Animated sparkle icon
            <div class="w-16 h-16 rounded-2xl bg-gradient-to-br from-amber-500/20 to-amber-600/10 flex items-center justify-center">
                <svg xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z"/>
                </svg>
            </div>
            <div class="space-y-2">
                <h2 class="text-xl font-semibold text-zinc-100">"Welcome to CoreML Studio"</h2>
                <p class="text-sm text-zinc-400 max-w-xs leading-relaxed">
                    "Explore, test, and compare Apple CoreML models in your browser"
                </p>
            </div>
        </div>
    }
}

#[component]
fn StepAddModel() -> impl IntoView {
    view! {
        <div class="flex flex-col items-center text-center space-y-5">
            // Model file icon
            <div class="w-16 h-16 rounded-2xl bg-gradient-to-br from-amber-500/20 to-amber-600/10 flex items-center justify-center">
                <svg xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m6.75 12l-3-3m0 0l-3 3m3-3v6m-1.5-15H5.625c-.621 0-1.125.504-1.125 1.125v17.25c0 .621.504 1.125 1.125 1.125h12.75c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9z"/>
                </svg>
            </div>
            <div class="space-y-2">
                <h2 class="text-xl font-semibold text-zinc-100">"Drop a .mlmodel file to get started"</h2>
                <p class="text-sm text-zinc-400 max-w-xs leading-relaxed">
                    "Models in "
                    <code class="text-xs bg-zinc-800 text-zinc-300 px-1.5 py-0.5 rounded">"~/CoreML-Models/"</code>
                    " are automatically detected"
                </p>
            </div>
            // Visual hint
            <div class="flex items-center gap-2 text-xs text-zinc-500">
                <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M4.5 10.5L12 3m0 0l7.5 7.5M12 3v18"/>
                </svg>
                <span>"Use the model switcher at the top to browse loaded models"</span>
            </div>
        </div>
    }
}

#[component]
fn StepExplore() -> impl IntoView {
    view! {
        <div class="flex flex-col items-center text-center space-y-5">
            // Chat icon
            <div class="w-16 h-16 rounded-2xl bg-gradient-to-br from-amber-500/20 to-amber-600/10 flex items-center justify-center">
                <svg xmlns="http://www.w3.org/2000/svg" class="w-8 h-8 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M7.5 8.25h9m-9 3H12m-9.75 1.51c0 1.6 1.123 2.994 2.707 3.227 1.129.166 2.27.293 3.423.379.35.026.67.21.865.501L12 21l2.755-4.133a1.14 1.14 0 01.865-.501 48.172 48.172 0 003.423-.379c1.584-.233 2.707-1.626 2.707-3.228V6.741c0-1.602-1.123-2.995-2.707-3.228A48.394 48.394 0 0012 3c-2.392 0-4.744.175-7.043.513C3.373 3.746 2.25 5.14 2.25 6.741v6.018z"/>
                </svg>
            </div>
            <div class="space-y-2">
                <h2 class="text-xl font-semibold text-zinc-100">"Chat with your model"</h2>
                <p class="text-sm text-zinc-400 max-w-xs leading-relaxed">
                    "Type a prompt, drop an image, or use these shortcuts"
                </p>
            </div>
            // Keyboard shortcuts
            <div class="w-full max-w-xs space-y-2 text-left">
                <div class="flex items-center justify-between bg-zinc-800/40 rounded-lg px-3 py-2">
                    <span class="text-sm text-zinc-300">"Open model switcher"</span>
                    <kbd class="text-xs bg-zinc-700/60 text-zinc-300 px-1.5 py-0.5 rounded font-mono">{"\u{2318}K"}</kbd>
                </div>
                <div class="flex items-center justify-between bg-zinc-800/40 rounded-lg px-3 py-2">
                    <span class="text-sm text-zinc-300">"New session"</span>
                    <kbd class="text-xs bg-zinc-700/60 text-zinc-300 px-1.5 py-0.5 rounded font-mono">{"\u{2318}N"}</kbd>
                </div>
                <div class="flex items-center justify-between bg-zinc-800/40 rounded-lg px-3 py-2">
                    <span class="text-sm text-zinc-300">"Model introspection"</span>
                    <kbd class="text-xs bg-zinc-700/60 text-zinc-300 px-1.5 py-0.5 rounded font-mono">{"\u{2318}I"}</kbd>
                </div>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// OnboardingFlow
// ---------------------------------------------------------------------------

/// A multi-step onboarding overlay for first-time users.
///
/// Reads `localStorage["onboarding_complete"]` to decide whether to show.
/// On completion the key is set to `"true"`.
///
/// Usage: `<OnboardingFlow />`
#[component]
pub fn OnboardingFlow() -> impl IntoView {
    let (visible, set_visible) = signal(false);
    let (step, set_step) = signal(0usize);

    const TOTAL_STEPS: usize = 3;

    // Check localStorage on mount (hydrate-only)
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            Effect::new(move || {
                if !is_onboarding_complete() {
                    set_visible.set(true);
                }
            });
        }
    }

    let on_next = move |_| {
        if step.get_untracked() < TOTAL_STEPS - 1 {
            set_step.update(|s| *s += 1);
        }
    };

    let on_back = move |_| {
        if step.get_untracked() > 0 {
            set_step.update(|s| *s -= 1);
        }
    };

    let on_finish = move |_| {
        mark_onboarding_complete();
        set_visible.set(false);
    };

    let on_skip = move |_| {
        mark_onboarding_complete();
        set_visible.set(false);
    };

    view! {
        <Show when=move || visible.get()>
            // Backdrop
            <div class="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 backdrop-blur-sm">
                // Card
                <div class="relative w-full max-w-md mx-4 bg-zinc-900 border border-zinc-800 rounded-2xl shadow-2xl overflow-hidden">
                    // Skip button (top-right)
                    <button
                        class="absolute top-3 right-3 text-xs text-zinc-500 hover:text-zinc-300 transition-colors cursor-pointer select-none"
                        on:click=on_skip
                    >
                        "Skip"
                    </button>

                    // Content area
                    <div class="px-8 pt-10 pb-6">
                        {move || match step.get() {
                            0 => view! { <StepWelcome /> }.into_any(),
                            1 => view! { <StepAddModel /> }.into_any(),
                            2 => view! { <StepExplore /> }.into_any(),
                            _ => view! { <StepWelcome /> }.into_any(),
                        }}
                    </div>

                    // Footer: dots + navigation
                    <div class="flex items-center justify-between px-8 pb-6">
                        <StepDots current=step total=TOTAL_STEPS />

                        <div class="flex items-center gap-2">
                            // Back button (hidden on first step)
                            <Show when=move || { step.get() > 0 }>
                                <button
                                    class="px-4 py-1.5 text-sm text-zinc-400 hover:text-zinc-200 transition-colors cursor-pointer select-none rounded-lg hover:bg-zinc-800/50"
                                    on:click=on_back
                                >
                                    "Back"
                                </button>
                            </Show>

                            // Next or Got it! button
                            {move || {
                                if step.get() < TOTAL_STEPS - 1 {
                                    view! {
                                        <button
                                            class="px-5 py-1.5 text-sm font-medium text-zinc-950 bg-amber-500 hover:bg-amber-400 rounded-lg transition-colors cursor-pointer select-none"
                                            on:click=on_next
                                        >
                                            "Next"
                                        </button>
                                    }.into_any()
                                } else {
                                    view! {
                                        <button
                                            class="px-5 py-1.5 text-sm font-medium text-zinc-950 bg-amber-500 hover:bg-amber-400 rounded-lg transition-colors cursor-pointer select-none"
                                            on:click=on_finish
                                        >
                                            "Got it!"
                                        </button>
                                    }.into_any()
                                }
                            }}
                        </div>
                    </div>
                </div>
            </div>
        </Show>
    }
}

// ---------------------------------------------------------------------------
// LandingHero
// ---------------------------------------------------------------------------

/// Feature card used within the LandingHero.
#[component]
fn FeatureCard(
    icon: &'static str,
    title: &'static str,
    description: &'static str,
) -> impl IntoView {
    // Map icon name to inline SVG
    let svg = match icon {
        "bolt" => view! {
            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 13.5l10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75z"/>
            </svg>
        }.into_any(),
        "columns" => view! {
            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M9 4.5v15m6-15v15m-10.875 0h15.75c.621 0 1.125-.504 1.125-1.125V5.625c0-.621-.504-1.125-1.125-1.125H4.125C3.504 4.5 3 5.004 3 5.625v12.75c0 .621.504 1.125 1.125 1.125z"/>
            </svg>
        }.into_any(),
        "magnifier" => view! {
            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"/>
            </svg>
        }.into_any(),
        _ => view! { <span /> }.into_any(),
    };

    view! {
        <div class="flex-1 bg-zinc-800/30 border border-zinc-800/50 rounded-xl p-4 space-y-2.5 hover:bg-zinc-800/50 transition-colors duration-200">
            <div class="w-9 h-9 rounded-lg bg-zinc-800/60 flex items-center justify-center">
                {svg}
            </div>
            <h3 class="text-sm font-medium text-zinc-200">{title}</h3>
            <p class="text-xs text-zinc-500 leading-relaxed">{description}</p>
        </div>
    }
}

/// A hero landing section shown in the empty state when no model is loaded.
///
/// Displays the app name, tagline, feature cards, and a CTA that opens the
/// model switcher via the `model_picker_open` context signal.
///
/// Usage: `<LandingHero />`
#[component]
pub fn LandingHero() -> impl IntoView {
    // Try to get the model-picker signal; fall back gracefully
    let maybe_set_picker = use_context::<crate::types::SetModelPickerOpen>();

    let on_get_started = move |_| {
        if let Some(set_picker) = maybe_set_picker {
            let set_picker = set_picker.0;
            set_picker.set(true);
        }
    };

    view! {
        <div class="flex-1 flex flex-col items-center justify-center px-6 select-none">
            // Subtle gradient background layer
            <div class="absolute inset-0 bg-gradient-to-b from-zinc-950 via-zinc-950 to-zinc-900/80 pointer-events-none" />

            <div class="relative z-10 flex flex-col items-center text-center space-y-8 max-w-2xl">
                // App icon
                <div class="w-14 h-14 rounded-2xl bg-gradient-to-br from-amber-500/20 to-amber-600/10 border border-amber-500/10 flex items-center justify-center">
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-7 h-7 text-amber-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z"/>
                    </svg>
                </div>

                // Title & tagline
                <div class="space-y-2">
                    <h1 class="text-2xl font-semibold text-zinc-100 tracking-tight">"CoreML Studio"</h1>
                    <p class="text-sm text-zinc-500">"See what your models see"</p>
                </div>

                // Feature cards
                <div class="flex flex-col sm:flex-row gap-3 w-full">
                    <FeatureCard
                        icon="bolt"
                        title="Instant Testing"
                        description="Load any CoreML model and test it in seconds"
                    />
                    <FeatureCard
                        icon="columns"
                        title="Visual Comparison"
                        description="Compare models side by side"
                    />
                    <FeatureCard
                        icon="magnifier"
                        title="Deep Inspection"
                        description="Understand what your model does under the hood"
                    />
                </div>

                // CTA
                <button
                    class="px-6 py-2.5 text-sm font-medium text-zinc-950 bg-amber-500 hover:bg-amber-400 rounded-xl transition-colors duration-150 cursor-pointer select-none shadow-lg shadow-amber-500/10"
                    on:click=on_get_started
                >
                    "Get Started"
                </button>
            </div>
        </div>
    }
}
