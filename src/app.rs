use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Meta, MetaTags, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    path,
};

use crate::components::chat::ChatView;
use crate::components::command_palette::CommandPalette;
use crate::components::comparison::ComparisonView;
use crate::components::export::ExportMenu;
use crate::components::onboarding::OnboardingFlow;
use crate::components::settings::{provide_persona_context, PersonaSettings};
use crate::components::toast::ToastProvider;
use crate::types::{
    LastInferenceMs, ModelInfo, ModelPickerOpen, SessionVersion, SetLastInferenceMs,
    SetModelPickerOpen, SetSessionVersion, SetShortcutNewSession, SetShowComparison,
    SetShowIntrospection, ShortcutNewSession, ShowComparison, ShowIntrospection,
};

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <link rel="stylesheet" id="leptos" href="/pkg/coreml-playground.css"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body class="bg-zinc-950 text-zinc-100 antialiased">
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Title text="CoreML Studio"/>
        <Meta name="color-scheme" content="dark"/>
        <ToastProvider>
            <Router>
                <main class="h-dvh flex">
                    <Routes fallback=|| "Not found.">
                        <Route path=path!("/") view=Home/>
                    </Routes>
                </main>
            </Router>
        </ToastProvider>
    }
}

#[component]
fn Home() -> impl IntoView {
    // Global signals
    let (active_model, set_active_model) = signal::<Option<ModelInfo>>(None);
    let (show_introspection, set_show_introspection) = signal(false);
    let (active_session_id, set_active_session_id) = signal::<Option<String>>(None);
    let (session_version, set_session_version) = signal(0u64);

    // Theme signal (CPO-13)
    let (theme, set_theme) = signal("dark".to_string());

    // Model picker open signal (CPO-19)
    let (model_picker_open, set_model_picker_open) = signal(false);

    // New-session shortcut counter (CPO-19)
    let (shortcut_new_session, set_shortcut_new_session) = signal(0u64);

    // Last inference timing (for introspection panel)
    let (last_inference_ms, set_last_inference_ms) = signal::<Option<u64>>(None);

    // Provide context to all children.
    // Unique types (no ambiguity — provided directly):
    provide_context(active_model); // ReadSignal<Option<ModelInfo>>
    provide_context(set_active_model); // WriteSignal<Option<ModelInfo>>
    provide_context(active_session_id); // ReadSignal<Option<String>>
    provide_context(set_active_session_id); // WriteSignal<Option<String>>
    provide_context(theme); // ReadSignal<String>
    provide_context(set_theme); // WriteSignal<String>

    // Newtype-wrapped signals (disambiguates same-typed contexts):
    provide_context(ShowIntrospection(show_introspection));
    provide_context(SetShowIntrospection(set_show_introspection));
    provide_context(ModelPickerOpen(model_picker_open));
    provide_context(SetModelPickerOpen(set_model_picker_open));
    provide_context(SessionVersion(session_version));
    provide_context(SetSessionVersion(set_session_version));
    provide_context(ShortcutNewSession(shortcut_new_session));
    provide_context(SetShortcutNewSession(set_shortcut_new_session));
    provide_context(LastInferenceMs(last_inference_ms));
    provide_context(SetLastInferenceMs(set_last_inference_ms));

    // Persona mode + complexity tier context
    provide_persona_context();

    // Comparison view visibility signal
    let (show_comparison, set_show_comparison) = signal(false);
    provide_context(ShowComparison(show_comparison));
    provide_context(SetShowComparison(set_show_comparison));

    // Export menu visibility
    let (show_export, set_show_export) = signal(false);

    // Client-side effect: sync theme class on <html> element (CPO-13)
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            Effect::new(move || {
                let current = theme.get();
                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    if let Some(html_el) = document.document_element() {
                        let cl = html_el.class_list();
                        let _ = cl.remove_2("dark", "light");
                        let _ = cl.add_1(&current);
                    }
                }
            });
        }
    }

    // Client-side global keyboard shortcuts (CPO-19)
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;

            let handler = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
                let meta = ev.meta_key() || ev.ctrl_key();

                if meta && ev.key() == "n" {
                    ev.prevent_default();
                    set_shortcut_new_session.update(|v| *v = v.wrapping_add(1));
                } else if meta && ev.key() == "i" {
                    ev.prevent_default();
                    set_show_introspection.update(|v| *v = !*v);
                } else if ev.key() == "Escape" {
                    set_show_introspection.set(false);
                    set_model_picker_open.set(false);
                }
            });

            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "keydown",
                    handler.as_ref().unchecked_ref(),
                );
            }
            // Leak the closure so it lives for the lifetime of the page
            handler.forget();
        }
    }

    view! {
        <div class="flex w-full h-full">
            // Session sidebar
            <crate::components::session_sidebar::SessionSidebar />

            // Main chat area
            <div class="flex-1 flex flex-col min-w-0">
                // Top bar with model lens
                <crate::components::model_switcher::ModelLens />

                // Chat messages
                <ChatView />

                // Adaptive input bar
                <crate::components::input_bar::InputBar />
            </div>

            // Theme toggle button (CPO-13)
            <button
                class="fixed top-3 right-3 z-50 p-1.5 rounded-lg text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors duration-150"
                on:click=move |_| {
                    set_theme.update(|t| {
                        *t = if *t == "dark" { "light".to_string() } else { "dark".to_string() };
                    });
                }
                title="Toggle theme"
            >
                {move || {
                    if theme.get() == "dark" {
                        // Sun icon — shown in dark mode (click to switch to light)
                        view! {
                            <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M12 3v2.25m6.364.386l-1.591 1.591M21 12h-2.25m-.386 6.364l-1.591-1.591M12 18.75V21m-4.773-4.227l-1.591 1.591M5.25 12H3m4.227-4.773L5.636 5.636M15.75 12a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0z"/>
                            </svg>
                        }.into_any()
                    } else {
                        // Moon icon — shown in light mode (click to switch to dark)
                        view! {
                            <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M21.752 15.002A9.718 9.718 0 0118 15.75c-5.385 0-9.75-4.365-9.75-9.75 0-1.33.266-2.597.748-3.752A9.753 9.753 0 003 11.25C3 16.635 7.365 21 12.75 21a9.753 9.753 0 009.002-5.998z"/>
                            </svg>
                        }.into_any()
                    }
                }}
            </button>

            // Introspection panel (conditional)
            <crate::components::introspection::IntrospectionPanel />

            // Persona settings (three-dot menu, top-right)
            <PersonaSettings />

            // Command palette overlay (Cmd+K)
            <CommandPalette />

            // Onboarding flow (first visit only)
            <OnboardingFlow />

            // Comparison view (toggled from command palette)
            <Show when=move || show_comparison.get()>
                <div class="fixed inset-0 z-[90] bg-zinc-950">
                    <ComparisonView />
                    <button
                        class="fixed top-3 left-3 z-[91] flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-400 hover:text-zinc-200 bg-zinc-800/80 hover:bg-zinc-700 border border-zinc-700/30 transition-colors duration-150"
                        on:click=move |_| set_show_comparison.set(false)
                    >
                        "Close comparison"
                    </button>
                </div>
            </Show>

            // Export menu (top bar area)
            <div class="fixed top-3 right-20 z-50">
                <button
                    class="p-1.5 rounded-lg text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors duration-150"
                    on:click=move |_| set_show_export.update(|v| *v = !*v)
                    title="Export session"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"/>
                    </svg>
                </button>
                <ExportMenu
                    session_id=Signal::derive(move || active_session_id.get().unwrap_or_default())
                    show=Signal::derive(move || show_export.get())
                    on_close=Callback::new(move |_| set_show_export.set(false))
                />
            </div>
        </div>
    }
}
