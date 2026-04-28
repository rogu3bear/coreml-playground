use leptos::prelude::*;

use crate::components::toast::{ToastLevel, ToastStore};
use crate::server::api::list_models;
use crate::types::ModelInfo;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
enum PaletteAction {
    LoadModel(String),
    NewSession,
    ExportSession,
    CompareModels,
    ToggleTheme,
    ToggleIntrospection,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PaletteCommand {
    id: String,
    label: String,
    category: String,
    shortcut: Option<String>,
    action: PaletteAction,
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// A VS Code-style command palette overlay activated by its own signal pair.
///
/// Provides `ReadSignal<bool>` and `WriteSignal<bool>` via context so that
/// parent components can open/close it externally.
#[component]
pub fn CommandPalette() -> impl IntoView {
    // Own open/close state, provided via context for external control.
    let (palette_open, set_palette_open) = signal(false);
    provide_context(palette_open);
    provide_context(set_palette_open);

    let (query, set_query) = signal(String::new());
    let (selected_index, set_selected_index) = signal::<usize>(0);
    let (recent_ids, set_recent_ids) = signal::<Vec<String>>(Vec::new());

    // Auto-focus input ref
    let input_ref = NodeRef::<leptos::html::Input>::new();

    // Fetch model list once (resource is cached across opens).
    let models = Resource::new(
        || (),
        |_| async move { list_models().await.unwrap_or_default() },
    );

    // Build the full command list from static commands + loaded models.
    let all_commands = Memo::new(move |_| {
        let mut cmds: Vec<PaletteCommand> = vec![
            PaletteCommand {
                id: "new-session".into(),
                label: "New session".into(),
                category: "Session".into(),
                shortcut: Some("\u{2318}N".into()),
                action: PaletteAction::NewSession,
            },
            PaletteCommand {
                id: "export-session".into(),
                label: "Export session".into(),
                category: "Session".into(),
                shortcut: None,
                action: PaletteAction::ExportSession,
            },
            PaletteCommand {
                id: "compare-models".into(),
                label: "Compare models".into(),
                category: "Models".into(),
                shortcut: None,
                action: PaletteAction::CompareModels,
            },
            PaletteCommand {
                id: "toggle-theme".into(),
                label: "Toggle theme".into(),
                category: "View".into(),
                shortcut: None,
                action: PaletteAction::ToggleTheme,
            },
            PaletteCommand {
                id: "toggle-introspection".into(),
                label: "Toggle introspection".into(),
                category: "View".into(),
                shortcut: Some("\u{2318}I".into()),
                action: PaletteAction::ToggleIntrospection,
            },
        ];

        // Append model-loading commands from the resource (if loaded).
        if let Some(model_list) = models.get() {
            for m in model_list {
                cmds.push(PaletteCommand {
                    id: format!("load-model-{}", m.id),
                    label: format!("Load model: {}", m.name),
                    category: "Models".into(),
                    shortcut: None,
                    action: PaletteAction::LoadModel(m.id.clone()),
                });
            }
        }

        cmds
    });

    // Filtered commands based on search query.
    let filtered_commands = Memo::new(move |_| {
        let q = query.get().to_lowercase();
        let cmds = all_commands.get();
        let recents = recent_ids.get();

        if q.is_empty() {
            // Show recent commands first (if any), then all commands.
            let mut recent_cmds: Vec<PaletteCommand> = Vec::new();
            let mut rest: Vec<PaletteCommand> = Vec::new();

            for cmd in &cmds {
                if recents.contains(&cmd.id) {
                    recent_cmds.push(cmd.clone());
                }
            }
            // Sort recent_cmds by position in the recents list (most recent first).
            recent_cmds.sort_by_key(|c| {
                recents
                    .iter()
                    .position(|r| r == &c.id)
                    .unwrap_or(usize::MAX)
            });

            for cmd in &cmds {
                if !recents.contains(&cmd.id) {
                    rest.push(cmd.clone());
                }
            }

            let mut result = recent_cmds;
            result.extend(rest);
            result
        } else {
            cmds.into_iter()
                .filter(|cmd| cmd.label.to_lowercase().contains(&q))
                .collect()
        }
    });

    // Reset selection when query or filtered list changes.
    Effect::new(move || {
        let _ = filtered_commands.get();
        set_selected_index.set(0);
    });

    // Focus input when palette opens (client-side only).
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            Effect::new(move || {
                if palette_open.get() {
                    // Clear search on open
                    set_query.set(String::new());
                    set_selected_index.set(0);
                    // Defer focus to next microtask so the input is rendered.
                    let input_ref = input_ref;
                    leptos::task::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(10).await;
                        if let Some(el) = input_ref.get() {
                            let _ = el.focus();
                        }
                    });
                }
            });
        }
    }

    // Execute a command.
    let execute = move |cmd: &PaletteCommand| {
        // Track as recent (keep last 5).
        let cmd_id = cmd.id.clone();
        set_recent_ids.update(|ids| {
            ids.retain(|id| id != &cmd_id);
            ids.insert(0, cmd_id);
            ids.truncate(5);
        });

        match &cmd.action {
            PaletteAction::LoadModel(model_id) => {
                let model_id = model_id.clone();
                // Grab the active model setter and load the model.
                let set_active_model = use_context::<WriteSignal<Option<ModelInfo>>>()
                    .expect("set_active_model context");
                let model_name = cmd.label.clone();
                leptos::task::spawn_local(async move {
                    match crate::server::api::load_model(model_id).await {
                        Ok(loaded) => {
                            set_active_model.set(Some(loaded));
                        }
                        Err(e) => {
                            let msg = format!("Failed to load {}: {}", model_name, e);
                            ToastStore::push(msg, ToastLevel::Error);
                        }
                    }
                });
            }
            PaletteAction::NewSession => {
                let set_shortcut = use_context::<crate::types::SetShortcutNewSession>()
                    .expect("SetShortcutNewSession context")
                    .0;
                set_shortcut.update(|v| *v = v.wrapping_add(1));
            }
            PaletteAction::ExportSession => {
                ToastStore::push(
                    "Use the export button in the top bar".into(),
                    ToastLevel::Info,
                );
            }
            PaletteAction::CompareModels => {
                let set_cmp = use_context::<crate::types::SetShowComparison>()
                    .expect("SetShowComparison context")
                    .0;
                set_cmp.set(true);
            }
            PaletteAction::ToggleTheme => {
                let set_theme = use_context::<WriteSignal<String>>().expect("set_theme context");
                set_theme.update(|t| {
                    *t = if *t == "dark" {
                        "light".to_string()
                    } else {
                        "dark".to_string()
                    };
                });
            }
            PaletteAction::ToggleIntrospection => {
                let set_show = use_context::<crate::types::SetShowIntrospection>()
                    .expect("SetShowIntrospection context")
                    .0;
                set_show.update(|v| *v = !*v);
            }
        }

        set_palette_open.set(false);
    };

    // Keyboard handler for the search input.
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
        let count = filtered_commands.get_untracked().len();

        match key.as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                set_selected_index.update(|i| {
                    if count > 0 {
                        *i = (*i + 1) % count;
                    }
                });
            }
            "ArrowUp" => {
                ev.prevent_default();
                set_selected_index.update(|i| {
                    if count > 0 {
                        *i = if *i == 0 { count - 1 } else { *i - 1 };
                    }
                });
            }
            "Enter" => {
                ev.prevent_default();
                let cmds = filtered_commands.get_untracked();
                let idx = selected_index.get_untracked();
                if let Some(cmd) = cmds.get(idx) {
                    execute(cmd);
                }
            }
            "Escape" => {
                ev.prevent_default();
                set_palette_open.set(false);
            }
            _ => {}
        }
    };

    // Client-side global keyboard shortcut to open palette.
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            use wasm_bindgen::prelude::*;
            use wasm_bindgen::JsCast;

            let handler = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
                let meta = ev.meta_key() || ev.ctrl_key();
                if meta && ev.key() == "k" {
                    ev.prevent_default();
                    set_palette_open.update(|v| *v = !*v);
                }
            });

            if let Some(window) = web_sys::window() {
                let _ = window.add_event_listener_with_callback(
                    "keydown",
                    handler.as_ref().unchecked_ref(),
                );
            }
            handler.forget();
        }
    }

    view! {
        {move || {
            if !palette_open.get() {
                return None;
            }

            let cmds = filtered_commands.get();
            let current_query = query.get();
            let has_recents = !recent_ids.get().is_empty() && current_query.is_empty();

            Some(view! {
                // Backdrop
                <div
                    class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm animate-fade-in"
                    on:click=move |_| set_palette_open.set(false)
                >
                    // Palette container — stop click propagation so clicking inside doesn't close
                    <div
                        class="mx-auto mt-[15vh] w-full max-w-[560px] bg-zinc-900 border border-zinc-700/60 rounded-xl shadow-2xl shadow-black/60 overflow-hidden animate-slide-down"
                        on:click=move |ev| ev.stop_propagation()
                    >
                        // Search input
                        <div class="flex items-center gap-3 px-4 py-3 border-b border-zinc-800/60">
                            // Magnifying glass icon
                            <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 text-zinc-500 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M21 21l-5.197-5.197m0 0A7.5 7.5 0 105.196 5.196a7.5 7.5 0 0010.607 10.607z"/>
                            </svg>
                            <input
                                node_ref=input_ref
                                type="text"
                                placeholder="Type a command..."
                                class="flex-1 bg-transparent text-sm text-zinc-200 placeholder-zinc-600 outline-none"
                                prop:value=move || query.get()
                                on:input=move |ev| {
                                    set_query.set(event_target_value(&ev));
                                }
                                on:keydown=on_keydown
                            />
                            <kbd class="hidden sm:inline-flex items-center px-1.5 py-0.5 text-[10px] font-medium text-zinc-500 bg-zinc-800/60 border border-zinc-700/40 rounded">
                                "ESC"
                            </kbd>
                        </div>

                        // Command list
                        <div class="max-h-[360px] overflow-y-auto py-1">
                            {if cmds.is_empty() {
                                view! {
                                    <div class="px-4 py-8 text-center text-sm text-zinc-500">
                                        "No results"
                                    </div>
                                }.into_any()
                            } else {
                                // Group commands by category and render
                                let mut grouped: Vec<(String, Vec<(usize, PaletteCommand)>)> = Vec::new();

                                if has_recents {
                                    let recents_list = recent_ids.get_untracked();
                                    let mut recent_group: Vec<(usize, PaletteCommand)> = Vec::new();
                                    for (idx, cmd) in cmds.iter().enumerate() {
                                        if recents_list.contains(&cmd.id) {
                                            recent_group.push((idx, cmd.clone()));
                                        }
                                    }
                                    if !recent_group.is_empty() {
                                        grouped.push(("Recent".into(), recent_group));
                                    }

                                    // Now group remaining by category
                                    let mut category_map: Vec<(String, Vec<(usize, PaletteCommand)>)> = Vec::new();
                                    for (idx, cmd) in cmds.iter().enumerate() {
                                        if recents_list.contains(&cmd.id) {
                                            continue;
                                        }
                                        if let Some(entry) = category_map.iter_mut().find(|(cat, _)| cat == &cmd.category) {
                                            entry.1.push((idx, cmd.clone()));
                                        } else {
                                            category_map.push((cmd.category.clone(), vec![(idx, cmd.clone())]));
                                        }
                                    }
                                    grouped.extend(category_map);
                                } else {
                                    // Group by category normally
                                    for (idx, cmd) in cmds.iter().enumerate() {
                                        if let Some(entry) = grouped.iter_mut().find(|(cat, _)| cat == &cmd.category) {
                                            entry.1.push((idx, cmd.clone()));
                                        } else {
                                            grouped.push((cmd.category.clone(), vec![(idx, cmd.clone())]));
                                        }
                                    }
                                }

                                let sel = selected_index.get();

                                view! {
                                    <div>
                                        {grouped.into_iter().map(|(category, items)| {
                                            view! {
                                                <div>
                                                    <div class="px-3 pt-2 pb-1">
                                                        <span class="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                                                            {category}
                                                        </span>
                                                    </div>
                                                    {items.into_iter().map(|(idx, cmd)| {
                                                        let is_selected = idx == sel;
                                                        let cmd_for_click = cmd.clone();
                                                        let shortcut_text = cmd.shortcut.clone();
                                                        view! {
                                                            <button
                                                                class="w-full flex items-center justify-between gap-2 px-3 py-2 text-left text-sm transition-colors duration-75"
                                                                class=(["bg-amber-500/10", "text-zinc-100"], is_selected)
                                                                class=(["text-zinc-300", "hover:bg-zinc-800/60"], !is_selected)
                                                                on:click=move |_| {
                                                                    execute(&cmd_for_click);
                                                                }
                                                                on:mouseenter=move |_| {
                                                                    set_selected_index.set(idx);
                                                                }
                                                            >
                                                                <span class="truncate">{cmd.label.clone()}</span>
                                                                {shortcut_text.map(|s| view! {
                                                                    <kbd class="flex-shrink-0 px-1.5 py-0.5 text-[10px] font-medium text-zinc-500 bg-zinc-800/60 border border-zinc-700/40 rounded">
                                                                        {s}
                                                                    </kbd>
                                                                })}
                                                            </button>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    </div>
                </div>
            })
        }}
    }
}
