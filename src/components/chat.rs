use leptos::prelude::*;

use crate::server::api::get_session_messages;
use crate::types::*;

// ---------------------------------------------------------------------------
// Copy-to-clipboard button (appears on hover via group-hover)
// ---------------------------------------------------------------------------

/// A small clipboard/checkmark button that copies text on click.
/// Hidden by default, shown on group-hover of the parent bubble.
#[component]
fn CopyButton(text: String) -> impl IntoView {
    let (copied, set_copied) = signal(false);

    let text_for_click = text.clone();
    let on_click = move |_| {
        let value = text_for_click.clone();
        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                if let Some(window) = web_sys::window() {
                    let clipboard = window.navigator().clipboard();
                    let promise = clipboard.write_text(&value);
                    drop(wasm_bindgen_futures::JsFuture::from(promise));
                    set_copied.set(true);
                    leptos::task::spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(1500).await;
                        set_copied.set(false);
                    });
                }
            } else {
                let _ = value;
                let _ = set_copied;
            }
        }
    };

    view! {
        <button
            on:click=on_click
            class="opacity-0 group-hover:opacity-100 transition-opacity duration-150 p-1 rounded hover:bg-zinc-700/50 text-zinc-500 hover:text-zinc-300 cursor-pointer select-none"
            title="Copy to clipboard"
        >
            {move || {
                if copied.get() {
                    // Checkmark icon
                    view! {
                        <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/>
                        </svg>
                    }.into_any()
                } else {
                    // Clipboard icon
                    view! {
                        <svg xmlns="http://www.w3.org/2000/svg" class="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                            <path stroke-linecap="round" stroke-linejoin="round" d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3"/>
                        </svg>
                    }.into_any()
                }
            }}
        </button>
    }
}

/// Extract copyable text from a message's content.
fn copyable_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(s) => s.clone(),
        MessageContent::Image { caption, .. } => caption.clone().unwrap_or_default(),
        MessageContent::ModelOutput(value) => {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        }
        MessageContent::Streaming { partial, .. } => partial.clone(),
        MessageContent::Batch(items) => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let json = serde_json::to_string_pretty(&item.output)
                    .unwrap_or_else(|_| item.output.to_string());
                format!("Image {}: {}", i + 1, json)
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

// ---------------------------------------------------------------------------
// Skeleton loading placeholders
// ---------------------------------------------------------------------------

/// Three alternating skeleton bubbles shown while messages load.
#[component]
fn SkeletonMessages() -> impl IntoView {
    view! {
        <div class="max-w-3xl mx-auto px-4 py-6 space-y-4">
            // Right-aligned (user-like)
            <div class="flex justify-end">
                <div class="skeleton skeleton-bubble max-w-[55%]"></div>
            </div>
            // Left-aligned (model-like)
            <div class="flex justify-start">
                <div class="skeleton skeleton-bubble max-w-[65%]"></div>
            </div>
            // Right-aligned (user-like)
            <div class="flex justify-end">
                <div class="skeleton skeleton-bubble max-w-[45%]"></div>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Bubble components
// ---------------------------------------------------------------------------

/// Renders a single chat bubble for a user message.
#[component]
fn UserBubble(message: ChatMessage) -> impl IntoView {
    let text = message.content.preview(4096);
    let copy_text = copyable_text(&message.content);
    view! {
        <div class="flex justify-end">
            <div class="group relative chat-bubble-user max-w-[70%] rounded-2xl rounded-br-sm px-4 py-2.5 bg-amber-600/90 text-zinc-50">
                <div class="absolute -left-8 top-1">
                    <CopyButton text=copy_text />
                </div>
                <p class="text-sm leading-relaxed whitespace-pre-wrap">{text}</p>
            </div>
        </div>
    }
}

/// Renders a single chat bubble for a model response.
#[component]
fn ModelBubble(message: ChatMessage) -> impl IntoView {
    let inference_ms = message.inference_ms;
    let content = message.content.clone();
    let copy_text = copyable_text(&message.content);

    view! {
        <div class="flex justify-start">
            <div class="group relative chat-bubble-model max-w-[80%] rounded-2xl rounded-bl-sm px-4 py-2.5 bg-zinc-800/80 text-zinc-200">
                <div class="absolute -right-8 top-1">
                    <CopyButton text=copy_text />
                </div>
                {move || match &content {
                    MessageContent::Text(s) => view! {
                        <p class="text-sm leading-relaxed whitespace-pre-wrap">{s.clone()}</p>
                    }.into_any(),
                    MessageContent::Image { data_base64, mime_type, caption } => {
                        let src = format!("data:{};base64,{}", mime_type, data_base64);
                        let cap = caption.clone();
                        view! {
                            <div class="space-y-2">
                                <img src=src alt="Model output" class="rounded-lg max-h-80 object-contain"/>
                                {cap.map(|c| view! {
                                    <p class="text-sm text-zinc-400">{c}</p>
                                })}
                            </div>
                        }.into_any()
                    }
                    MessageContent::ModelOutput(value) => {
                        let formatted = serde_json::to_string_pretty(value)
                            .unwrap_or_else(|_| value.to_string());
                        view! {
                            <pre class="text-xs font-mono bg-zinc-900 rounded-lg p-3 overflow-x-auto text-zinc-300 leading-relaxed">
                                <code>{formatted}</code>
                            </pre>
                        }.into_any()
                    }
                    MessageContent::Streaming { partial, done } => {
                        let text = partial.clone();
                        let is_done = *done;
                        view! {
                            <p class="text-sm leading-relaxed whitespace-pre-wrap">
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
                        let total_ms: u64 = items
                            .iter()
                            .filter_map(|item| item.inference_ms)
                            .sum();
                        let count = items.len();
                        let rows = items
                            .iter()
                            .map(|item| {
                                let src = format!("data:{};base64,{}", item.mime_type, item.image_base64);
                                let formatted = serde_json::to_string_pretty(&item.output)
                                    .unwrap_or_else(|_| item.output.to_string());
                                let ms = item.inference_ms;
                                view! {
                                    <div class="grid grid-cols-[60px_1fr] gap-3 items-start">
                                        <img
                                            src=src
                                            alt="Batch image"
                                            class="w-[60px] h-[60px] rounded-lg object-cover border border-zinc-700/50"
                                        />
                                        <div class="min-w-0">
                                            <pre class="text-xs font-mono bg-zinc-900 rounded-lg p-2 overflow-x-auto text-zinc-300 leading-relaxed">
                                                <code>{formatted}</code>
                                            </pre>
                                            {ms.map(|ms| {
                                                let display = latency_display(ms);
                                                view! {
                                                    <p class="text-xs text-zinc-600 mt-1 select-none">{display}</p>
                                                }
                                            })}
                                        </div>
                                    </div>
                                }
                            })
                            .collect_view();
                        view! {
                            <div class="space-y-3">
                                <p class="text-xs font-medium text-zinc-400 select-none">
                                    {format!("Batch results \u{2014} {} images", count)}
                                </p>
                                <div class="space-y-2">
                                    {rows}
                                </div>
                                <p class="text-xs text-zinc-500 pt-1 border-t border-zinc-700/30 select-none">
                                    {format!("Total: {}", latency_display(total_ms))}
                                </p>
                            </div>
                        }.into_any()
                    }
                }}
                {inference_ms.map(|ms| {
                    let display = latency_display(ms);
                    view! {
                        <p class="text-xs text-zinc-500 mt-1.5 select-none">{display}</p>
                    }
                })}
            </div>
        </div>
    }
}

/// Renders an image message from the user (with optional caption).
#[component]
fn UserImageBubble(message: ChatMessage) -> impl IntoView {
    let content = message.content.clone();
    let copy_text = copyable_text(&message.content);
    view! {
        <div class="flex justify-end">
            <div class="group relative chat-bubble-user max-w-[70%] rounded-2xl rounded-br-sm px-4 py-2.5 bg-amber-600/90 text-zinc-50">
                <div class="absolute -left-8 top-1">
                    <CopyButton text=copy_text />
                </div>
                {match &content {
                    MessageContent::Image { data_base64, mime_type, caption } => {
                        let src = format!("data:{};base64,{}", mime_type, data_base64);
                        let cap = caption.clone();
                        view! {
                            <div class="space-y-2">
                                <img src=src alt="Uploaded image" class="rounded-lg max-h-64 object-contain"/>
                                {cap.map(|c| view! {
                                    <p class="text-sm text-zinc-100">{c}</p>
                                })}
                            </div>
                        }.into_any()
                    }
                    _ => view! { <span></span> }.into_any(),
                }}
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Empty state
// ---------------------------------------------------------------------------

/// Empty state shown when there are no messages yet.
#[component]
fn EmptyState() -> impl IntoView {
    let active_model =
        use_context::<ReadSignal<Option<ModelInfo>>>().expect("active_model context");

    let model_name = move || {
        active_model
            .get()
            .map(|m| m.name.clone())
            .unwrap_or_default()
    };

    let example_prompts = move || {
        active_model.get().map(|m| match m.model_type {
            ModelType::Text => vec![
                "Summarize this text for me...",
                "Translate the following paragraph...",
                "What is the sentiment of this review?",
            ],
            ModelType::Vision => vec![
                "Describe what you see in this image",
                "Classify the objects in this photo",
                "What breed of dog is this?",
            ],
            ModelType::Multimodal => vec![
                "Describe this image in detail",
                "What text do you see in this photo?",
                "Answer a question about this image...",
            ],
            ModelType::Audio => vec![
                "Transcribe this audio clip",
                "What language is being spoken?",
                "Classify the sound in this recording",
            ],
            ModelType::Unknown => vec!["Send a prompt to the model", "Try an example input"],
        })
    };

    view! {
        <div class="flex-1 flex flex-col items-center justify-center px-6 text-center space-y-6 select-none">
            {move || {
                if active_model.get().is_some() {
                    view! {
                        <div class="space-y-4">
                            <h2 class="text-xl font-medium text-zinc-300">{model_name()}</h2>
                            <p class="text-sm text-zinc-500">Start a conversation</p>
                        </div>
                        <div class="flex flex-col gap-2 max-w-sm w-full">
                            {example_prompts().unwrap_or_default().into_iter().map(|prompt| {
                                let p = prompt.to_string();
                                view! {
                                    <button class="text-left text-sm text-zinc-400 hover:text-zinc-200 bg-zinc-800/40 hover:bg-zinc-800/70 rounded-xl px-4 py-3 transition-colors duration-150">
                                        {p}
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                } else {
                    // No model selected -- richer instructional empty state
                    view! {
                        <div class="space-y-5 max-w-sm">
                            <div class="w-12 h-12 rounded-2xl bg-zinc-800/50 flex items-center justify-center mx-auto">
                                <svg xmlns="http://www.w3.org/2000/svg" class="w-6 h-6 text-zinc-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M9.813 15.904L9 18.75l-.813-2.846a4.5 4.5 0 00-3.09-3.09L2.25 12l2.846-.813a4.5 4.5 0 003.09-3.09L9 5.25l.813 2.846a4.5 4.5 0 003.09 3.09L15.75 12l-2.846.813a4.5 4.5 0 00-3.09 3.09zM18.259 8.715L18 9.75l-.259-1.035a3.375 3.375 0 00-2.455-2.456L14.25 6l1.036-.259a3.375 3.375 0 002.455-2.456L18 2.25l.259 1.035a3.375 3.375 0 002.455 2.456L21.75 6l-1.036.259a3.375 3.375 0 00-2.455 2.456z"/>
                                </svg>
                            </div>
                            <div class="space-y-1">
                                <h2 class="text-lg font-medium text-zinc-400">
                                    "CoreML Playground"
                                </h2>
                                <p class="text-sm text-zinc-500">
                                    "Explore your CoreML models"
                                </p>
                            </div>
                            <div class="text-left space-y-2.5 bg-zinc-800/30 rounded-xl px-4 py-3">
                                <div class="flex items-start gap-2.5">
                                    <span class="shrink-0 mt-0.5 w-5 h-5 rounded-full bg-zinc-700/60 text-zinc-400 text-xs flex items-center justify-center font-medium">"1"</span>
                                    <p class="text-sm text-zinc-400">"Select a model from the lens above"</p>
                                </div>
                                <div class="flex items-start gap-2.5">
                                    <span class="shrink-0 mt-0.5 w-5 h-5 rounded-full bg-zinc-700/60 text-zinc-400 text-xs flex items-center justify-center font-medium">"2"</span>
                                    <p class="text-sm text-zinc-400">"Type a prompt or drop an image"</p>
                                </div>
                                <div class="flex items-start gap-2.5">
                                    <span class="shrink-0 mt-0.5 w-5 h-5 rounded-full bg-zinc-700/60 text-zinc-400 text-xs flex items-center justify-center font-medium">"3"</span>
                                    <p class="text-sm text-zinc-400">
                                        "See model introspection with "
                                        <kbd class="text-xs bg-zinc-700/50 text-zinc-300 px-1 py-0.5 rounded">{"\u{2318}I"}</kbd>
                                    </p>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Message dispatch
// ---------------------------------------------------------------------------

/// Renders a single message based on role and content type.
#[component]
fn MessageItem(message: ChatMessage) -> impl IntoView {
    let role = message.role.clone();
    let is_image = matches!(message.content, MessageContent::Image { .. });

    match role {
        MessageRole::User if is_image => view! { <UserImageBubble message=message /> }.into_any(),
        MessageRole::User => view! { <UserBubble message=message /> }.into_any(),
        MessageRole::Model | MessageRole::System => {
            view! { <ModelBubble message=message /> }.into_any()
        }
    }
}

// ---------------------------------------------------------------------------
// Main chat view
// ---------------------------------------------------------------------------

/// The main chat view. Displays messages for the active session or an empty state.
#[component]
pub fn ChatView() -> impl IntoView {
    let active_session_id =
        use_context::<ReadSignal<Option<String>>>().expect("active_session_id context");
    let session_version = use_context::<crate::types::SessionVersion>()
        .expect("SessionVersion context")
        .0;

    // Fetch messages when session changes (or when version bumps after sending)
    let messages = Resource::new(
        move || (active_session_id.get(), session_version.get()),
        move |(sid, _version)| async move {
            match sid {
                Some(id) => get_session_messages(id).await.unwrap_or_default(),
                None => Vec::new(),
            }
        },
    );

    // Reference to scroll container for auto-scroll
    let scroll_ref = NodeRef::<leptos::html::Div>::new();

    // Auto-scroll to bottom whenever messages change (client-side only)
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            Effect::new(move || {
                // Track the messages resource so this fires on every update
                let _ = messages.get();

                // Scroll after the DOM updates
                if let Some(el) = scroll_ref.get() {
                    request_animation_frame(move || {
                        let el: &web_sys::Element = &el;
                        el.set_scroll_top(el.scroll_height());
                    });
                }
            });
        }
    }

    view! {
        <div
            node_ref=scroll_ref
            class="flex-1 overflow-y-auto scroll-smooth"
        >
            <Suspense fallback=move || view! {
                <SkeletonMessages />
            }>
                {move || {
                    let msgs = messages.get().unwrap_or_default();
                    if msgs.is_empty() {
                        view! { <EmptyState /> }.into_any()
                    } else {
                        view! {
                            <div class="max-w-3xl mx-auto px-4 py-6 space-y-4">
                                {msgs.into_iter().map(|msg| {
                                    view! { <MessageItem message=msg /> }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }}
            </Suspense>
        </div>
    }
}
