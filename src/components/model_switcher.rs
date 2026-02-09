use leptos::prelude::*;

use crate::components::toast::{ToastLevel, ToastStore};
use crate::server::api::{list_models, load_model};
use crate::types::*;

/// Formats a byte count into a human-readable file size string.
fn format_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Returns the Tailwind classes for a model type badge.
fn badge_classes(model_type: &ModelType) -> &'static str {
    match model_type {
        ModelType::Text => "bg-blue-500/15 text-blue-400 ring-blue-500/20",
        ModelType::Vision => "bg-purple-500/15 text-purple-400 ring-purple-500/20",
        ModelType::Multimodal => "bg-amber-500/15 text-amber-400 ring-amber-500/20",
        ModelType::Audio => "bg-green-500/15 text-green-400 ring-green-500/20",
        ModelType::Unknown => "bg-zinc-500/15 text-zinc-400 ring-zinc-500/20",
    }
}

/// Returns the Tailwind classes for an active model type badge (with glow effect).
fn badge_classes_active(model_type: &ModelType) -> &'static str {
    match model_type {
        ModelType::Text => "bg-blue-500/25 text-blue-300 ring-blue-400/40 shadow-[0_0_8px_rgba(59,130,246,0.3)]",
        ModelType::Vision => "bg-purple-500/25 text-purple-300 ring-purple-400/40 shadow-[0_0_8px_rgba(168,85,247,0.3)]",
        ModelType::Multimodal => "bg-amber-500/25 text-amber-300 ring-amber-400/40 shadow-[0_0_8px_rgba(245,158,11,0.3)]",
        ModelType::Audio => "bg-green-500/25 text-green-300 ring-green-400/40 shadow-[0_0_8px_rgba(34,197,94,0.3)]",
        ModelType::Unknown => "bg-zinc-500/25 text-zinc-300 ring-zinc-400/40 shadow-[0_0_8px_rgba(161,161,170,0.2)]",
    }
}

/// A small colored badge showing the model type label.
#[component]
fn TypeBadge(
    model_type: ModelType,
    #[prop(optional)] active: bool,
) -> impl IntoView {
    let classes = if active {
        badge_classes_active(&model_type)
    } else {
        badge_classes(&model_type)
    };
    let label = model_type.label();
    view! {
        <span class=format!(
            "inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium ring-1 ring-inset transition-all duration-200 {}",
            classes
        )>
            {label}
        </span>
    }
}

/// A brief summary of the model's I/O ports.
fn port_summary(ports: &[PortInfo]) -> String {
    if ports.is_empty() {
        return "none".into();
    }
    ports
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A single model card in the dropdown picker.
#[component]
fn ModelCard(
    model: ModelInfo,
    on_select: Callback<ModelInfo>,
    #[prop(into)] loading_id: Signal<Option<String>>,
) -> impl IntoView {
    let model_for_click = model.clone();
    let card_id = model.id.clone();
    let name = model.name.clone();
    let model_type = model.model_type.clone();
    let input_text = port_summary(&model.input_schema);
    let output_text = port_summary(&model.output_schema);
    let size_text = format_file_size(model.file_size_bytes);

    let is_loading = Memo::new({
        let card_id = card_id.clone();
        move |_| loading_id.get().as_deref() == Some(card_id.as_str())
    });

    view! {
        <button
            class="w-full text-left p-3 rounded-xl bg-zinc-800/40 hover:bg-zinc-800/80 border border-zinc-700/30 hover:border-zinc-600/50 transition-all duration-150 group relative overflow-hidden"
            class:animate-pulse=move || is_loading.get()
            class:pointer-events-none=move || is_loading.get()
            on:click=move |_| {
                on_select.run(model_for_click.clone());
            }
        >
            <div class="flex items-start justify-between gap-2">
                <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2 mb-1">
                        {move || {
                            if is_loading.get() {
                                view! {
                                    <div class="w-3.5 h-3.5 border-2 border-zinc-700 border-t-amber-500 rounded-full animate-spin"></div>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}
                        <span class="text-sm font-medium text-zinc-200 truncate group-hover:text-zinc-50 transition-colors">{name}</span>
                        <TypeBadge model_type=model_type />
                    </div>
                    <div class="text-xs text-zinc-500 space-y-0.5">
                        <p class="truncate">"In: " {input_text}</p>
                        <p class="truncate">"Out: " {output_text}</p>
                    </div>
                </div>
                <span class="text-xs text-zinc-600 whitespace-nowrap pt-0.5">{size_text}</span>
            </div>
        </button>
    }
}

/// Help section shown at the bottom of the model picker dropdown.
#[component]
fn ModelImportHelp() -> impl IntoView {
    view! {
        <div class="text-xs text-zinc-500 px-4 py-2 border-t border-zinc-800/50 flex items-center gap-1.5">
            // Folder icon (inline SVG)
            <svg
                xmlns="http://www.w3.org/2000/svg"
                class="w-3.5 h-3.5 flex-shrink-0 text-zinc-600"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="1.5"
            >
                <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M2.25 12.75V12A2.25 2.25 0 014.5 9.75h15A2.25 2.25 0 0121.75 12v.75m-8.69-6.44l-2.12-2.12a1.5 1.5 0 00-1.061-.44H4.5A2.25 2.25 0 002.25 6v12a2.25 2.25 0 002.25 2.25h15A2.25 2.25 0 0021.75 18V9a2.25 2.25 0 00-2.25-2.25h-5.379a1.5 1.5 0 01-1.06-.44z"
                />
            </svg>
            <span>"Place .mlmodel or .mlpackage files in the models/ directory"</span>
        </div>
    }
}

/// The top bar of the main area. Shows the active model or a "Select a model" prompt.
/// Clicking it opens a dropdown/drawer listing all available models.
#[component]
pub fn ModelLens() -> impl IntoView {
    let active_model =
        use_context::<ReadSignal<Option<ModelInfo>>>().expect("active_model context");
    let set_active_model =
        use_context::<WriteSignal<Option<ModelInfo>>>().expect("set_active_model context");
    let set_show_introspection =
        use_context::<crate::types::SetShowIntrospection>().expect("SetShowIntrospection context").0;

    // Wire up the Cmd+K model_picker_open context signal from app.rs
    let picker_open_read =
        use_context::<crate::types::ModelPickerOpen>().expect("ModelPickerOpen context").0;
    let picker_open_write =
        use_context::<crate::types::SetModelPickerOpen>().expect("SetModelPickerOpen context").0;

    let (dropdown_open, set_dropdown_open) = signal(false);
    let (loading_model, set_loading_model) = signal::<Option<String>>(None);
    let (loading_slow, set_loading_slow) = signal(false);

    // Track a "generation" counter so we can tie the 2-second slow timer to the
    // correct load operation (avoids stale closures from previous loads).
    let (load_gen, set_load_gen) = signal(0u64);

    // Sync external picker_open_read into local dropdown_open (client-side only)
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            Effect::new(move || {
                let external = picker_open_read.get();
                if external != dropdown_open.get_untracked() {
                    set_dropdown_open.set(external);
                }
            });

            // Sync local dropdown_open back to external context when it changes
            Effect::new(move || {
                let local = dropdown_open.get();
                if local != picker_open_read.get_untracked() {
                    picker_open_write.set(local);
                }
            });
        } else {
            // Suppress unused variable warnings in SSR mode
            let _ = (picker_open_read, picker_open_write);
        }
    }

    let models = Resource::new(|| (), |_| async move {
        list_models().await.unwrap_or_default()
    });

    // Derive a signal from loading_model for passing into ModelCard
    let loading_id_signal: Signal<Option<String>> = loading_model.into();

    let on_select = Callback::new(move |model: ModelInfo| {
        let model_id = model.id.clone();
        let model_name = model.name.clone();
        set_loading_model.set(Some(model_id.clone()));
        set_loading_slow.set(false);
        set_dropdown_open.set(false);

        // Bump the generation counter
        let gen = load_gen.get_untracked().wrapping_add(1);
        set_load_gen.set(gen);

        // Start the 2-second slow loading timer (client-side only)
        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                leptos::task::spawn_local({
                    async move {
                        gloo_timers::future::TimeoutFuture::new(2000).await;
                        // Only show "slow" message if we're still loading the same operation
                        if load_gen.get_untracked() == gen && loading_model.get_untracked().is_some() {
                            set_loading_slow.set(true);
                        }
                    }
                });
            }
        }

        leptos::task::spawn_local(async move {
            match load_model(model_id).await {
                Ok(loaded) => {
                    set_active_model.set(Some(loaded));
                }
                Err(e) => {
                    let msg = format!("Failed to load {}: {}", model_name, e);
                    ToastStore::push(msg, ToastLevel::Error);
                }
            }
            set_loading_model.set(None);
            set_loading_slow.set(false);
        });
    });

    view! {
        <div class="model-lens relative border-b border-zinc-800/60">
            // Active model bar
            <div class="flex items-center justify-between px-4 py-3">
                <button
                    class="flex items-center gap-2 min-w-0 group"
                    on:click=move |_| set_dropdown_open.update(|v| *v = !*v)
                >
                    {move || {
                        if let Some(ref loading_id) = loading_model.get() {
                            let display_name = loading_id.clone();
                            view! {
                                <div class="flex items-center gap-2">
                                    <div class="w-4 h-4 border-2 border-zinc-700 border-t-amber-500 rounded-full animate-spin"></div>
                                    <span class="text-sm text-zinc-400">
                                        {format!("Loading {}...", display_name)}
                                    </span>
                                    {move || {
                                        if loading_slow.get() {
                                            Some(view! {
                                                <span class="text-xs text-zinc-600 ml-1">"This may take a moment for large models..."</span>
                                            })
                                        } else {
                                            None
                                        }
                                    }}
                                </div>
                            }.into_any()
                        } else if let Some(model) = active_model.get() {
                            let model_type = model.model_type.clone();
                            view! {
                                <div class="flex items-center gap-2 relative">
                                    <span class="text-sm font-medium text-zinc-200 model-loaded-name">{model.name.clone()}</span>
                                    <span class="model-loaded-badge">
                                        <TypeBadge model_type=model_type active=true />
                                    </span>
                                    <svg
                                        xmlns="http://www.w3.org/2000/svg"
                                        class="w-3.5 h-3.5 text-zinc-500 group-hover:text-zinc-300 transition-transform duration-150"
                                        class:rotate-180=move || dropdown_open.get()
                                        fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
                                    >
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5"/>
                                    </svg>
                                    // Sweep line under active model display
                                    <div class="model-loaded-sweep absolute bottom-0 left-0 right-0"></div>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="flex items-center gap-2">
                                    <span class="text-sm text-zinc-500 animate-pulse">"Select a model"</span>
                                    <svg
                                        xmlns="http://www.w3.org/2000/svg"
                                        class="w-3.5 h-3.5 text-zinc-600"
                                        fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
                                    >
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5"/>
                                    </svg>
                                </div>
                            }.into_any()
                        }
                    }}
                </button>

                // Info toggle button (only visible when a model is loaded)
                {move || active_model.get().map(|_| view! {
                    <button
                        class="p-1.5 rounded-lg text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors duration-150"
                        on:click=move |_| set_show_introspection.update(|v| *v = !*v)
                        title="Model details"
                    >
                        <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M11.25 11.25l.041-.02a.75.75 0 011.063.852l-.708 2.836a.75.75 0 001.063.853l.041-.021M21 12a9 9 0 11-18 0 9 9 0 0118 0zm-9-3.75h.008v.008H12V8.25z"/>
                        </svg>
                    </button>
                })}
            </div>

            // Dropdown model picker
            {move || {
                if dropdown_open.get() {
                    Some(view! {
                        // Backdrop
                        <div
                            class="fixed inset-0 z-30"
                            on:click=move |_| set_dropdown_open.set(false)
                        ></div>
                        // Panel
                        <div class="absolute top-full left-0 right-0 z-40 mx-4 mt-1 bg-zinc-900 border border-zinc-700/50 rounded-xl shadow-xl shadow-black/40 max-h-80 overflow-y-auto animate-fade-in">
                            <div class="p-2">
                                <Suspense fallback=move || view! {
                                    <div class="flex items-center justify-center py-8">
                                        <div class="w-5 h-5 border-2 border-zinc-700 border-t-amber-500 rounded-full animate-spin"></div>
                                    </div>
                                }>
                                    {move || {
                                        let model_list = models.get().unwrap_or_default();
                                        if model_list.is_empty() {
                                            view! {
                                                <p class="text-sm text-zinc-500 text-center py-6">"No models found"</p>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <div class="space-y-1.5">
                                                    {model_list.into_iter().map(|m| {
                                                        view! { <ModelCard model=m on_select=on_select loading_id=loading_id_signal /> }
                                                    }).collect_view()}
                                                </div>
                                            }.into_any()
                                        }
                                    }}
                                </Suspense>
                            </div>
                            // Model import help section (USER-1)
                            <ModelImportHelp />
                        </div>
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}
