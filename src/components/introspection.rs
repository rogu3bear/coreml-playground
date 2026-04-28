use leptos::prelude::*;

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

/// Transforms raw tensor shape information into a human-readable description.
/// Pattern-matches common ML shapes and port types to produce friendly text.
fn humanize_shape(port: &PortInfo) -> String {
    let name_lower = port.name.to_lowercase();
    let pt_lower = port.port_type.to_lowercase();

    // Text inputs (string type or text-like names)
    if pt_lower == "string" || pt_lower.contains("text") {
        return "Text input (variable length)".to_string();
    }

    if let Some(ref shape) = port.shape {
        match shape.as_slice() {
            // [1, 3, H, W] — NCHW color image
            [1, 3, h, w] if *h > 0 && *w > 0 => {
                return format!("{}\u{00d7}{} RGB Image", h, w);
            }
            // [1, H, W, 3] — NHWC color image
            [1, h, w, 3] if *h > 0 && *w > 0 => {
                return format!("{}\u{00d7}{} RGB Image", h, w);
            }
            // [1, 1, H, W] — NCHW grayscale image
            [1, 1, h, w] if *h > 0 && *w > 0 => {
                return format!("{}\u{00d7}{} Grayscale Image", h, w);
            }
            // [1, H, W, 1] — NHWC grayscale image
            [1, h, w, 1] if *h > 0 && *w > 0 => {
                return format!("{}\u{00d7}{} Grayscale Image", h, w);
            }
            // Generic image type with 4-dim shape
            [1, c, h, w] if pt_lower.contains("image") && *c > 0 && *h > 0 && *w > 0 => {
                return format!("{}\u{00d7}{} Color Image", h, w);
            }
            // [1, N] where N=1000 and name hints at classification
            [1, 1000] => {
                if name_lower.contains("label")
                    || name_lower.contains("class")
                    || name_lower.contains("prob")
                    || name_lower.contains("score")
                {
                    return "1000 categories".to_string();
                }
                return "1000-dim vector".to_string();
            }
            // [1] — scalar output
            [1] => {
                return "Single value".to_string();
            }
            // [1, N] — embedding or feature vector
            [1, n] if *n > 1 => {
                if name_lower.contains("embed")
                    || name_lower.contains("feature")
                    || name_lower.contains("hidden")
                    || (64..=4096).contains(n)
                {
                    return format!("{}-dim embedding", n);
                }
                return format!("{}-dim vector", n);
            }
            _ => {}
        }

        // Image port type without matching shape above
        if pt_lower.contains("image") {
            return "Image input".to_string();
        }
    } else {
        // No shape information
        if pt_lower.contains("image") {
            return "Image input".to_string();
        }
        if pt_lower.contains("dictionary") {
            return "Key-value pairs".to_string();
        }
    }

    // Fallback: use port type
    match &port.shape {
        Some(s) => {
            let dims: Vec<String> = s
                .iter()
                .map(|d| {
                    if *d < 0 {
                        "?".to_string()
                    } else {
                        d.to_string()
                    }
                })
                .collect();
            format!("{} [{}]", port.port_type, dims.join("\u{00d7}"))
        }
        None => port.port_type.clone(),
    }
}

/// Returns the CSS class for a latency indicator dot.
fn latency_dot_class(ms: u64) -> &'static str {
    match ms {
        0..=199 => "w-2 h-2 rounded-full bg-green-400 inline-block",
        200..=999 => "w-2 h-2 rounded-full bg-amber-400 inline-block",
        _ => "w-2 h-2 rounded-full bg-red-400 inline-block",
    }
}

/// Renders a port info badge showing name, type, optional shape, and humanized description.
#[component]
fn PortBadge(port: PortInfo) -> impl IntoView {
    let shape_text = port.shape.as_ref().map(|dims| {
        let parts: Vec<String> = dims
            .iter()
            .map(|d| {
                if *d < 0 {
                    "?".to_string()
                } else {
                    d.to_string()
                }
            })
            .collect();
        format!("[{}]", parts.join(" x "))
    });

    let humanized = humanize_shape(&port);

    view! {
        <div class="port-badge flex items-center gap-2 rounded-lg bg-zinc-800/50 border border-zinc-700/30 px-3 py-2">
            <div class="min-w-0 flex-1">
                <p class="text-xs font-medium text-zinc-300 truncate">{port.name}</p>
                <p class="text-[12px] text-zinc-200 mt-0.5">{humanized}</p>
                <div class="flex items-center gap-1.5 mt-0.5">
                    <span class="text-[11px] text-zinc-500 font-mono">{port.port_type}</span>
                    {shape_text.map(|s| view! {
                        <span class="text-[11px] text-zinc-600 font-mono">{s}</span>
                    })}
                </div>
            </div>
        </div>
    }
}

/// A section header for the port lists.
#[component]
fn PortSection(title: &'static str, ports: Vec<PortInfo>) -> impl IntoView {
    view! {
        <div class="space-y-2">
            <h4 class="text-xs font-semibold uppercase tracking-wider text-zinc-500">{title}</h4>
            {if ports.is_empty() {
                view! {
                    <p class="text-xs text-zinc-600 italic">"None"</p>
                }.into_any()
            } else {
                view! {
                    <div class="space-y-1.5">
                        {ports.into_iter().map(|p| {
                            view! { <PortBadge port=p /> }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}
        </div>
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

/// Performance section showing inference latency with a colored indicator dot.
#[component]
fn PerformanceSection(last_inference_ms: ReadSignal<Option<u64>>) -> impl IntoView {
    view! {
        <div class="space-y-1">
            <h4 class="text-xs font-semibold uppercase tracking-wider text-zinc-500">"Performance"</h4>
            {move || {
                match last_inference_ms.get() {
                    None => {
                        view! {
                            <p class="text-xs text-zinc-600 italic">"No inference data yet"</p>
                        }.into_any()
                    }
                    Some(ms) => {
                        let dot_class = latency_dot_class(ms);
                        let display = latency_display(ms);
                        view! {
                            <div class="flex items-center gap-2">
                                <span class=dot_class></span>
                                <span class="text-sm text-zinc-400 font-mono">{display}</span>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}

/// Feedback section with thumbs-up / thumbs-down rating buttons.
#[component]
fn FeedbackSection(model_id: String) -> impl IntoView {
    // Track the current rating: None = not rated, Some(true) = thumbs up, Some(false) = thumbs down.
    let (rating, set_rating) = signal::<Option<bool>>(None);
    // Track which model this rating is for, so it resets when the model changes.
    let (rated_model_id, set_rated_model_id) = signal(model_id.clone());

    view! {
        <div class="space-y-2">
            <h4 class="text-xs font-semibold uppercase tracking-wider text-zinc-500">"Feedback"</h4>
            {move || {
                let current_model = model_id.clone();
                // Reset rating if the model has changed
                if rated_model_id.get() != current_model {
                    set_rated_model_id.set(current_model);
                    set_rating.set(None);
                }

                let current_rating = rating.get();

                view! {
                    <div class="flex items-center gap-3">
                        // Thumbs up button
                        <button
                            class={move || {
                                if current_rating == Some(true) {
                                    "p-1.5 rounded-md text-amber-500 transition-colors"
                                } else {
                                    "p-1.5 rounded-md text-zinc-500 hover:text-amber-500 transition-colors"
                                }
                            }}
                            on:click=move |_| set_rating.set(Some(true))
                            title="Thumbs up"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M6.633 10.25c.806 0 1.533-.446 2.031-1.08a9.041 9.041 0 0 1 2.861-2.4c.723-.384 1.35-.956 1.653-1.715a4.498 4.498 0 0 0 .322-1.672V3a.75.75 0 0 1 .75-.75 2.25 2.25 0 0 1 2.25 2.25c0 1.152-.26 2.243-.723 3.218-.266.558.107 1.282.725 1.282h3.126c1.026 0 1.945.694 2.054 1.715.045.422.068.85.068 1.285a11.95 11.95 0 0 1-2.649 7.521c-.388.482-.987.729-1.605.729H14.23c-.483 0-.964-.078-1.423-.23l-3.114-1.04a4.501 4.501 0 0 0-1.423-.23H5.904m.729-5.305V13.5c0-.621-.504-1.125-1.125-1.125H4.125C3.504 12.375 3 12.879 3 13.5v6.75c0 .621.504 1.125 1.125 1.125h1.383c.621 0 1.125-.504 1.125-1.125V17.25m.729-5.305l-.729 5.305"/>
                            </svg>
                        </button>
                        // Thumbs down button
                        <button
                            class={move || {
                                if current_rating == Some(false) {
                                    "p-1.5 rounded-md text-amber-500 transition-colors"
                                } else {
                                    "p-1.5 rounded-md text-zinc-500 hover:text-amber-500 transition-colors"
                                }
                            }}
                            on:click=move |_| set_rating.set(Some(false))
                            title="Thumbs down"
                        >
                            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M7.498 15.25H4.372c-1.026 0-1.945-.694-2.054-1.715A12.137 12.137 0 0 1 2.25 12c0-2.848.992-5.464 2.649-7.521C5.287 3.997 5.886 3.75 6.504 3.75h4.369c.483 0 .964.078 1.423.23l3.114 1.04a4.501 4.501 0 0 0 1.423.23h1.294M7.498 15.25c.618 0 .991.724.725 1.282A7.471 7.471 0 0 0 7.5 19.5a2.25 2.25 0 0 0 2.25 2.25.75.75 0 0 0 .75-.75v-.633c0-.573.11-1.14.322-1.672.304-.76.93-1.33 1.653-1.715a9.04 9.04 0 0 0 2.86-2.4c.498-.634 1.226-1.08 2.032-1.08h.384"/>
                            </svg>
                        </button>
                        // Confirmation text
                        {move || {
                            if rating.get().is_some() {
                                view! {
                                    <span class="text-xs text-zinc-400 animate-fade-in">"Thanks!"</span>
                                }.into_any()
                            } else {
                                view! {
                                    <span class="text-xs text-zinc-600">"Rate this model\u{2019}s output"</span>
                                }.into_any()
                            }
                        }}
                    </div>
                }
            }}
        </div>
    }
}

/// Side panel for inspecting the currently loaded model. Slides in from the right
/// when `show_introspection` context signal is true.
#[component]
pub fn IntrospectionPanel() -> impl IntoView {
    let show = use_context::<crate::types::ShowIntrospection>()
        .expect("ShowIntrospection context")
        .0;
    let set_show = use_context::<crate::types::SetShowIntrospection>()
        .expect("SetShowIntrospection context")
        .0;
    let active_model =
        use_context::<ReadSignal<Option<ModelInfo>>>().expect("active_model context");

    let last_inference_ms = use_context::<crate::types::LastInferenceMs>()
        .expect("LastInferenceMs context")
        .0;

    view! {
        {move || {
            if !show.get() {
                return view! { <div class="hidden"></div> }.into_any();
            }

            match active_model.get() {
                None => {
                    view! { <div class="hidden"></div> }.into_any()
                }
                Some(model) => {
                    let model_type = model.model_type.clone();
                    let badge = badge_classes(&model_type);
                    let label = model_type.label();
                    let name = model.name.clone();
                    let description = model.description.clone();
                    let author = model.author.clone();
                    let inputs = model.input_schema.clone();
                    let outputs = model.output_schema.clone();
                    let size_text = format_file_size(model.file_size_bytes);
                    let model_id_for_feedback = model.id.clone();

                    view! {
                        <aside class="introspection-panel w-80 border-l border-zinc-800/60 bg-zinc-900/30 flex flex-col animate-slide-in overflow-hidden">
                            // Header
                            <div class="flex items-center justify-between px-4 py-3 border-b border-zinc-800/40">
                                <h3 class="text-sm font-medium text-zinc-300">"Model Details"</h3>
                                <button
                                    class="p-1 rounded-md text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800/50 transition-colors"
                                    on:click=move |_| set_show.set(false)
                                    title="Close"
                                >
                                    <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                        <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                                    </svg>
                                </button>
                            </div>

                            // Content
                            <div class="flex-1 overflow-y-auto px-4 py-4 space-y-5">
                                // Name + type
                                <div class="space-y-1.5">
                                    <h2 class="text-base font-semibold text-zinc-100">{name}</h2>
                                    <span class=format!(
                                        "inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium ring-1 ring-inset {}",
                                        badge
                                    )>
                                        {label}
                                    </span>
                                </div>

                                // Description
                                {description.map(|d| view! {
                                    <div class="space-y-1">
                                        <h4 class="text-xs font-semibold uppercase tracking-wider text-zinc-500">"Description"</h4>
                                        <p class="text-sm text-zinc-400 leading-relaxed">{d}</p>
                                    </div>
                                })}

                                // Author
                                {author.map(|a| view! {
                                    <div class="space-y-1">
                                        <h4 class="text-xs font-semibold uppercase tracking-wider text-zinc-500">"Author"</h4>
                                        <p class="text-sm text-zinc-400">{a}</p>
                                    </div>
                                })}

                                // Inputs
                                <PortSection title="Inputs" ports=inputs />

                                // Outputs
                                <PortSection title="Outputs" ports=outputs />

                                // File size
                                <div class="space-y-1">
                                    <h4 class="text-xs font-semibold uppercase tracking-wider text-zinc-500">"File Size"</h4>
                                    <p class="text-sm text-zinc-400 font-mono">{size_text}</p>
                                </div>

                                // Performance (CPO-20)
                                <PerformanceSection last_inference_ms=last_inference_ms />

                                // Feedback (CPO-22)
                                <FeedbackSection model_id=model_id_for_feedback />
                            </div>
                        </aside>
                    }.into_any()
                }
            }
        }}
    }
}
