use leptos::prelude::*;

use crate::components::toast::{ToastLevel, ToastStore};
use crate::server::api::{create_session, list_models, send_message};
use crate::types::*;

// ---------------------------------------------------------------------------
// Comparison result entry (one row of side-by-side output)
// ---------------------------------------------------------------------------

/// A single comparison result: the user's input and two model outputs.
#[derive(Clone, Debug)]
struct ComparisonEntry {
    id: String,
    input_text: String,
    left_response: Option<ChatMessage>,
    right_response: Option<ChatMessage>,
    left_loading: bool,
    right_loading: bool,
}

// ---------------------------------------------------------------------------
// Model selector dropdown (self-contained, for each pane)
// ---------------------------------------------------------------------------

/// A dropdown to pick a model for one side of the comparison.
#[component]
fn PaneModelSelector(
    label: &'static str,
    selected: RwSignal<Option<String>>,
    selected_name: RwSignal<Option<String>>,
    models: Signal<Vec<ModelInfo>>,
) -> impl IntoView {
    let (open, set_open) = signal(false);

    view! {
        <div class="relative">
            <button
                class="flex items-center gap-2 px-3 py-2 w-full text-left bg-zinc-800/50 hover:bg-zinc-800/80 border border-zinc-700/40 rounded-lg transition-colors duration-150"
                on:click=move |_| set_open.update(|v| *v = !*v)
            >
                <span class="text-xs font-medium text-zinc-400 uppercase tracking-wider shrink-0">{label}</span>
                <span class="text-sm text-zinc-200 truncate flex-1">
                    {move || selected_name.get().unwrap_or_else(|| "Select model...".to_string())}
                </span>
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    class="w-3.5 h-3.5 text-zinc-500 shrink-0 transition-transform duration-150"
                    class:rotate-180=move || open.get()
                    fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"
                >
                    <path stroke-linecap="round" stroke-linejoin="round" d="M19.5 8.25l-7.5 7.5-7.5-7.5"/>
                </svg>
            </button>

            // Dropdown list
            {move || {
                if open.get() {
                    Some(view! {
                        // Backdrop
                        <div
                            class="fixed inset-0 z-30"
                            on:click=move |_| set_open.set(false)
                        ></div>
                        // Panel
                        <div class="absolute top-full left-0 right-0 z-40 mt-1 bg-zinc-900 border border-zinc-700/50 rounded-xl shadow-xl shadow-black/40 max-h-56 overflow-y-auto animate-fade-in">
                            <div class="p-1.5 space-y-0.5">
                                {move || {
                                    let model_list = models.get();
                                    if model_list.is_empty() {
                                        view! {
                                            <p class="text-sm text-zinc-500 text-center py-4">"No models found"</p>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div>
                                                {model_list.into_iter().map(|m| {
                                                    let id = m.id.clone();
                                                    let name = m.name.clone();
                                                    let id_click = id.clone();
                                                    let name_click = name.clone();
                                                    let is_active_bg = {
                                                        let id = id.clone();
                                                        move || selected.get().as_deref() == Some(id.as_str())
                                                    };
                                                    let is_active_text = {
                                                        let id = id.clone();
                                                        move || selected.get().as_deref() == Some(id.as_str())
                                                    };
                                                    view! {
                                                        <button
                                                            class="w-full text-left px-3 py-2 rounded-lg text-sm transition-colors duration-100 hover:bg-zinc-800/80"
                                                            class:bg-zinc-800=is_active_bg
                                                            class:text-amber-400=is_active_text
                                                            class:text-zinc-300={
                                                                let id = id.clone();
                                                                move || selected.get().as_deref() != Some(id.as_str())
                                                            }
                                                            on:click=move |_| {
                                                                selected.set(Some(id_click.clone()));
                                                                selected_name.set(Some(name_click.clone()));
                                                                set_open.set(false);
                                                            }
                                                        >
                                                            {name}
                                                        </button>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    }
                                }}
                            </div>
                        </div>
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Response bubble (renders one side's output)
// ---------------------------------------------------------------------------

/// Renders a single model response in a comparison pane.
#[component]
fn ComparisonBubble(msg: Option<ChatMessage>, loading: bool) -> impl IntoView {
    view! {
        <div class="px-3 py-2">
            {move || {
                if loading {
                    view! {
                        <div class="flex items-center gap-2 text-sm text-zinc-500">
                            <div class="w-3.5 h-3.5 border-2 border-zinc-700 border-t-amber-500 rounded-full animate-spin"></div>
                            <span>"Running inference..."</span>
                        </div>
                    }.into_any()
                } else if let Some(ref message) = msg {
                    let inference_ms = message.inference_ms;
                    match &message.content {
                        MessageContent::Text(s) => {
                            let s = s.clone();
                            view! {
                                <div>
                                    <p class="text-sm leading-relaxed whitespace-pre-wrap text-zinc-200">{s}</p>
                                    {inference_ms.map(|ms| {
                                        let display = latency_display(ms);
                                        view! {
                                            <p class="text-xs text-zinc-500 mt-1.5 select-none">{display}</p>
                                        }
                                    })}
                                </div>
                            }.into_any()
                        }
                        MessageContent::ModelOutput(value) => {
                            let formatted = serde_json::to_string_pretty(value)
                                .unwrap_or_else(|_| value.to_string());
                            view! {
                                <div>
                                    <pre class="text-xs font-mono bg-zinc-900 rounded-lg p-3 overflow-x-auto text-zinc-300 leading-relaxed">
                                        <code>{formatted}</code>
                                    </pre>
                                    {inference_ms.map(|ms| {
                                        let display = latency_display(ms);
                                        view! {
                                            <p class="text-xs text-zinc-500 mt-1.5 select-none">{display}</p>
                                        }
                                    })}
                                </div>
                            }.into_any()
                        }
                        MessageContent::Image { data_base64, mime_type, caption } => {
                            let src = format!("data:{};base64,{}", mime_type, data_base64);
                            let cap = caption.clone();
                            view! {
                                <div class="space-y-2">
                                    <img src=src alt="Model output" class="rounded-lg max-h-48 object-contain"/>
                                    {cap.map(|c| view! {
                                        <p class="text-sm text-zinc-400">{c}</p>
                                    })}
                                    {inference_ms.map(|ms| {
                                        let display = latency_display(ms);
                                        view! {
                                            <p class="text-xs text-zinc-500 mt-1.5 select-none">{display}</p>
                                        }
                                    })}
                                </div>
                            }.into_any()
                        }
                        MessageContent::Streaming { partial, done } => {
                            let text = partial.clone();
                            let is_done = *done;
                            view! {
                                <p class="text-sm leading-relaxed whitespace-pre-wrap text-zinc-200">
                                    {text}
                                    {if !is_done {
                                        Some(view! { <span class="streaming-cursor inline-block w-0.5 h-4 bg-amber-400 animate-pulse ml-0.5 align-text-bottom"></span> })
                                    } else {
                                        None
                                    }}
                                </p>
                            }.into_any()
                        }
                        MessageContent::Batch(items) => {
                            let count = items.len();
                            let rows = items
                                .iter()
                                .map(|item| {
                                    let src = format!("data:{};base64,{}", item.mime_type, item.image_base64);
                                    let formatted = serde_json::to_string_pretty(&item.output)
                                        .unwrap_or_else(|_| item.output.to_string());
                                    view! {
                                        <div class="grid grid-cols-[48px_1fr] gap-2 items-start">
                                            <img src=src alt="Batch image" class="w-12 h-12 rounded-lg object-cover border border-zinc-700/50"/>
                                            <pre class="text-xs font-mono bg-zinc-900 rounded-lg p-2 overflow-x-auto text-zinc-300 leading-relaxed">
                                                <code>{formatted}</code>
                                            </pre>
                                        </div>
                                    }
                                })
                                .collect_view();
                            view! {
                                <div class="space-y-2">
                                    <p class="text-xs font-medium text-zinc-400 select-none">{format!("Batch: {} images", count)}</p>
                                    {rows}
                                    {inference_ms.map(|ms| {
                                        let display = latency_display(ms);
                                        view! {
                                            <p class="text-xs text-zinc-500 mt-1.5 select-none">{display}</p>
                                        }
                                    })}
                                </div>
                            }.into_any()
                        }
                    }
                } else {
                    view! {
                        <p class="text-sm text-zinc-600 italic">"Waiting for input..."</p>
                    }.into_any()
                }
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Main ComparisonView component
// ---------------------------------------------------------------------------

/// Split-pane comparison view for running the same input against two models
/// side by side. Self-contained: manages its own state and session creation.
///
/// Usage: `<ComparisonView />`
#[component]
pub fn ComparisonView() -> impl IntoView {
    // Internal comparing toggle — the parent can read this if needed but
    // we do not require any external context for it.
    let (_comparing, set_comparing) = signal(true);

    // Model selections
    let left_model: RwSignal<Option<String>> = RwSignal::new(None);
    let right_model: RwSignal<Option<String>> = RwSignal::new(None);
    let left_model_name: RwSignal<Option<String>> = RwSignal::new(None);
    let right_model_name: RwSignal<Option<String>> = RwSignal::new(None);

    // Comparison results history
    let entries: RwSignal<Vec<ComparisonEntry>> = RwSignal::new(Vec::new());

    // Input state
    let (input_text, set_input_text) = signal(String::new());
    let (sending, set_sending) = signal(false);

    let textarea_ref = NodeRef::<leptos::html::Textarea>::new();

    // Session IDs for each side (auto-created when first message is sent)
    let left_session: RwSignal<Option<String>> = RwSignal::new(None);
    let right_session: RwSignal<Option<String>> = RwSignal::new(None);

    // Fetch available models
    let models_resource = Resource::new(
        || (),
        |_| async move { list_models().await.unwrap_or_default() },
    );

    let models_signal: Signal<Vec<ModelInfo>> =
        Signal::derive(move || models_resource.get().unwrap_or_default());

    // Scroll container ref
    let scroll_ref = NodeRef::<leptos::html::Div>::new();

    // Auto-scroll to bottom on new entries
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            Effect::new(move || {
                let _ = entries.get();
                if let Some(el) = scroll_ref.get() {
                    request_animation_frame(move || {
                        let el: &web_sys::Element = &el;
                        el.set_scroll_top(el.scroll_height());
                    });
                }
            });
        }
    }

    // Resize textarea to fit content
    let resize_textarea = move || {
        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                if let Some(el) = textarea_ref.get() {
                    use wasm_bindgen::JsCast;
                    let html_el: &web_sys::HtmlElement = el.unchecked_ref();
                    html_el.style().set_property("height", "auto").ok();
                    let scroll_h = el.scroll_height();
                    let clamped = scroll_h.clamp(40, 192);
                    html_el.style()
                        .set_property("height", &format!("{}px", clamped))
                        .ok();
                }
            }
        }
    };

    // Submit: send the same input to both models
    let submit = move || {
        if sending.get() {
            return;
        }

        let text = input_text.get().trim().to_string();
        if text.is_empty() {
            return;
        }

        let l_model = left_model.get();
        let r_model = right_model.get();

        if l_model.is_none() || r_model.is_none() {
            ToastStore::push(
                "Select both models before comparing".to_string(),
                ToastLevel::Warning,
            );
            return;
        }

        let l_model_id = l_model.unwrap();
        let r_model_id = r_model.unwrap();

        set_sending.set(true);
        set_input_text.set(String::new());

        // Reset textarea height
        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                if let Some(el) = textarea_ref.get() {
                    use wasm_bindgen::JsCast;
                    let html_el: &web_sys::HtmlElement = el.unchecked_ref();
                    html_el.style().set_property("height", "auto").ok();
                }
            }
        }

        // Create a placeholder entry
        let entry_id = uuid::Uuid::new_v4().to_string();
        let placeholder = ComparisonEntry {
            id: entry_id.clone(),
            input_text: text.clone(),
            left_response: None,
            right_response: None,
            left_loading: true,
            right_loading: true,
        };
        entries.update(|list| list.push(placeholder));

        let input = InferenceInput::Text(text.clone());

        // Spawn left model inference
        let entry_id_left = entry_id.clone();
        let l_model_id_clone = l_model_id.clone();
        let input_left = input.clone();
        leptos::task::spawn_local(async move {
            // Ensure we have a session for the left model
            let sid = if let Some(id) = left_session.get() {
                id
            } else {
                match create_session(l_model_id_clone.clone()).await {
                    Ok(session) => {
                        let sid = session.id.clone();
                        left_session.set(Some(session.id));
                        sid
                    }
                    Err(e) => {
                        ToastStore::push(format!("Left session error: {e}"), ToastLevel::Error);
                        entries.update(|list| {
                            if let Some(entry) = list.iter_mut().find(|e| e.id == entry_id_left) {
                                entry.left_loading = false;
                            }
                        });
                        return;
                    }
                }
            };

            match send_message(sid, input_left).await {
                Ok(msg) => {
                    entries.update(|list| {
                        if let Some(entry) = list.iter_mut().find(|e| e.id == entry_id_left) {
                            entry.left_response = Some(msg);
                            entry.left_loading = false;
                        }
                    });
                }
                Err(e) => {
                    ToastStore::push(format!("Left model error: {e}"), ToastLevel::Error);
                    entries.update(|list| {
                        if let Some(entry) = list.iter_mut().find(|e| e.id == entry_id_left) {
                            entry.left_loading = false;
                        }
                    });
                }
            }

            // Check if both sides are done
            let both_done = entries
                .get()
                .iter()
                .all(|e| !e.left_loading && !e.right_loading);
            if both_done {
                set_sending.set(false);
            }
        });

        // Spawn right model inference
        let entry_id_right = entry_id.clone();
        let r_model_id_clone = r_model_id.clone();
        let input_right = input.clone();
        leptos::task::spawn_local(async move {
            // Ensure we have a session for the right model
            let sid = if let Some(id) = right_session.get() {
                id
            } else {
                match create_session(r_model_id_clone.clone()).await {
                    Ok(session) => {
                        let sid = session.id.clone();
                        right_session.set(Some(session.id));
                        sid
                    }
                    Err(e) => {
                        ToastStore::push(format!("Right session error: {e}"), ToastLevel::Error);
                        entries.update(|list| {
                            if let Some(entry) = list.iter_mut().find(|e| e.id == entry_id_right) {
                                entry.right_loading = false;
                            }
                        });
                        return;
                    }
                }
            };

            match send_message(sid, input_right).await {
                Ok(msg) => {
                    entries.update(|list| {
                        if let Some(entry) = list.iter_mut().find(|e| e.id == entry_id_right) {
                            entry.right_response = Some(msg);
                            entry.right_loading = false;
                        }
                    });
                }
                Err(e) => {
                    ToastStore::push(format!("Right model error: {e}"), ToastLevel::Error);
                    entries.update(|list| {
                        if let Some(entry) = list.iter_mut().find(|e| e.id == entry_id_right) {
                            entry.right_loading = false;
                        }
                    });
                }
            }

            // Check if both sides are done
            let both_done = entries
                .get()
                .iter()
                .all(|e| !e.left_loading && !e.right_loading);
            if both_done {
                set_sending.set(false);
            }
        });
    };

    // Keyboard handler for textarea
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            submit();
        }
    };

    let on_input = move |ev: leptos::ev::Event| {
        let value = event_target_value(&ev);
        set_input_text.set(value);
        resize_textarea();
    };

    let on_submit_click = move |_: leptos::ev::MouseEvent| {
        submit();
    };

    // Check if both models are selected
    let both_selected = move || left_model.get().is_some() && right_model.get().is_some();

    view! {
        <div class="flex flex-col h-full w-full">
            // Top bar with "Back to single" button
            <div class="flex items-center justify-between px-4 py-2.5 border-b border-zinc-800/60 bg-zinc-950/80 backdrop-blur-sm">
                <div class="flex items-center gap-2">
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 text-amber-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M7.5 21L3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"/>
                    </svg>
                    <span class="text-sm font-medium text-zinc-300">"Compare Models"</span>
                </div>
                <button
                    class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium text-zinc-400 hover:text-zinc-200 bg-zinc-800/40 hover:bg-zinc-800/80 border border-zinc-700/30 transition-colors duration-150"
                    on:click=move |_| set_comparing.set(false)
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 15L3 9m0 0l6-6M3 9h12a6 6 0 010 12h-3"/>
                    </svg>
                    "Back to single"
                </button>
            </div>

            // Model selectors row
            <div class="grid grid-cols-2 gap-px bg-zinc-800/30">
                <div class="bg-zinc-950 p-2">
                    <PaneModelSelector
                        label="A"
                        selected=left_model
                        selected_name=left_model_name
                        models=models_signal
                    />
                </div>
                <div class="bg-zinc-950 p-2">
                    <PaneModelSelector
                        label="B"
                        selected=right_model
                        selected_name=right_model_name
                        models=models_signal
                    />
                </div>
            </div>

            // Results area (scrollable)
            <div
                node_ref=scroll_ref
                class="flex-1 overflow-y-auto scroll-smooth"
            >
                {move || {
                    let current_entries = entries.get();
                    if current_entries.is_empty() {
                        // Empty state
                        view! {
                            <div class="flex-1 flex flex-col items-center justify-center h-full px-6 text-center space-y-4 select-none py-20">
                                <div class="w-12 h-12 rounded-2xl bg-zinc-800/50 flex items-center justify-center">
                                    <svg xmlns="http://www.w3.org/2000/svg" class="w-6 h-6 text-zinc-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M7.5 21L3 16.5m0 0L7.5 12M3 16.5h13.5m0-13.5L21 7.5m0 0L16.5 12M21 7.5H7.5"/>
                                    </svg>
                                </div>
                                <div class="space-y-1">
                                    <h2 class="text-lg font-medium text-zinc-400">"Side-by-side comparison"</h2>
                                    <p class="text-sm text-zinc-500">"Select two models and type a prompt to compare their outputs"</p>
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="divide-y divide-zinc-800/40">
                                {current_entries.into_iter().map(|entry| {
                                    let input_text = entry.input_text.clone();
                                    let left_resp = entry.left_response.clone();
                                    let right_resp = entry.right_response.clone();
                                    let left_load = entry.left_loading;
                                    let right_load = entry.right_loading;
                                    view! {
                                        <div class="space-y-0">
                                            // User input (full width)
                                            <div class="px-4 py-2.5 bg-zinc-900/30">
                                                <div class="flex items-start gap-2 max-w-3xl mx-auto">
                                                    <span class="shrink-0 mt-0.5 w-5 h-5 rounded-full bg-amber-600/20 text-amber-400 text-xs flex items-center justify-center font-medium">"Q"</span>
                                                    <p class="text-sm text-zinc-300 whitespace-pre-wrap">{input_text}</p>
                                                </div>
                                            </div>
                                            // Side-by-side responses
                                            <div class="grid grid-cols-2 gap-px bg-zinc-800/20">
                                                <div class="bg-zinc-950/50 min-h-[60px]">
                                                    <ComparisonBubble msg=left_resp loading=left_load />
                                                </div>
                                                <div class="bg-zinc-950/50 min-h-[60px]">
                                                    <ComparisonBubble msg=right_resp loading=right_load />
                                                </div>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            // Shared input bar at the bottom
            <div class="border-t border-zinc-800/60 bg-zinc-950/80 backdrop-blur-sm">
                <div class="max-w-3xl mx-auto px-4 py-3">
                    <div class="flex items-end gap-2">
                        <textarea
                            node_ref=textarea_ref
                            class="flex-1 bg-zinc-800/40 border border-zinc-700/30 rounded-xl px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 resize-none focus:outline-none focus:ring-1 focus:ring-amber-500/30 focus:border-amber-500/30 transition-all duration-150 disabled:opacity-40 disabled:cursor-not-allowed"
                            style="height: 40px"
                            placeholder=move || {
                                if !both_selected() {
                                    "Select both models to compare..."
                                } else {
                                    "Type a prompt to send to both models..."
                                }
                            }
                            disabled=move || !both_selected() || sending.get()
                            prop:value=move || input_text.get()
                            on:input=on_input
                            on:keydown=on_keydown
                            rows="1"
                        ></textarea>

                        // Send button
                        <button
                            class="flex-shrink-0 p-2.5 rounded-xl bg-amber-600 hover:bg-amber-500 disabled:bg-zinc-800 disabled:text-zinc-600 text-zinc-950 transition-colors duration-150 disabled:cursor-not-allowed"
                            disabled=move || {
                                !both_selected() || sending.get() || input_text.get().trim().is_empty()
                            }
                            on:click=on_submit_click
                        >
                            {move || {
                                if sending.get() {
                                    view! {
                                        <div class="w-4 h-4 border-2 border-zinc-600 border-t-zinc-300 rounded-full animate-spin"></div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 12L3.269 3.126A59.768 59.768 0 0121.485 12 59.77 59.77 0 013.27 20.876L5.999 12zm0 0h7.5"/>
                                        </svg>
                                    }.into_any()
                                }
                            }}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    }
}
