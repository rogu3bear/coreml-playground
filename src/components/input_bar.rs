use leptos::prelude::*;

use crate::server::api::{create_session, send_message};
use crate::types::*;

/// The adaptive input bar that morphs based on the active model's input type.
/// - No model loaded: disabled input with placeholder
/// - Text model: auto-growing textarea with Enter to submit
/// - Vision/Multimodal model: image drop zone + text prompt
/// - (USER-12) Drop multiple images for batch inference
#[component]
pub fn InputBar() -> impl IntoView {
    let active_model =
        use_context::<ReadSignal<Option<ModelInfo>>>().expect("active_model context");
    let active_session_id =
        use_context::<ReadSignal<Option<String>>>().expect("active_session_id context");
    let set_active_session_id =
        use_context::<WriteSignal<Option<String>>>().expect("set_active_session_id context");
    let set_session_version = use_context::<crate::types::SetSessionVersion>()
        .expect("SetSessionVersion context")
        .0;

    let (input_text, set_input_text) = signal(String::new());
    let (sending, set_sending) = signal(false);
    let (image_data, set_image_data) = signal::<Option<ImagePreview>>(None);
    // (USER-12) Batch images signal — holds 2+ images when the user drops multiple files
    let batch_images = RwSignal::new(Vec::<ImagePreview>::new());
    let (drag_active, set_drag_active) = signal(false);

    let textarea_ref = NodeRef::<leptos::html::Textarea>::new();
    let container_ref = NodeRef::<leptos::html::Div>::new();
    let file_input_ref = NodeRef::<leptos::html::Input>::new();

    let accepts_image = move || {
        active_model
            .get()
            .map(|m| m.model_type.accepts_image())
            .unwrap_or(false)
    };

    // Whether any image is queued (single or batch)
    let has_any_image = move || image_data.get().is_some() || !batch_images.get().is_empty();

    // (CPO-8) Contextual placeholder based on model type
    let placeholder_text = move || match active_model.get() {
        None => "Select a model to begin...",
        Some(m) => match m.model_type {
            ModelType::Text => "Type a message...",
            ModelType::Vision => "Drop an image or describe what you see...",
            ModelType::Multimodal => "Type or drop an image...",
            ModelType::Audio => "Audio models process audio files...",
            ModelType::Unknown => "Type a message...",
        },
    };

    // (USER-22) With auto-create, the input is only truly disabled when no model is loaded
    let is_model_missing = move || active_model.get().is_none();

    // Resize textarea to fit content (client-side only)
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

    // (USER-3) Clipboard paste handler for images
    let on_paste = move |ev: leptos::ev::ClipboardEvent| {
        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                use wasm_bindgen::JsCast;

                // Access clipboardData from the paste event
                let clipboard_data: wasm_bindgen::JsValue = ev.clone().into();
                let cd = js_sys::Reflect::get(
                    &clipboard_data,
                    &wasm_bindgen::JsValue::from_str("clipboardData"),
                ).ok();

                if let Some(cd) = cd {
                    if !cd.is_undefined() && !cd.is_null() {
                        // Get the items property
                        let items = js_sys::Reflect::get(
                            &cd,
                            &wasm_bindgen::JsValue::from_str("items"),
                        ).ok();

                        if let Some(items_val) = items {
                            if !items_val.is_undefined() && !items_val.is_null() {
                                let length = js_sys::Reflect::get(
                                    &items_val,
                                    &wasm_bindgen::JsValue::from_str("length"),
                                ).ok()
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0) as u32;

                                for i in 0..length {
                                    let item = js_sys::Reflect::get_u32(&items_val, i).ok();
                                    if let Some(item_val) = item {
                                        let kind = js_sys::Reflect::get(
                                            &item_val,
                                            &wasm_bindgen::JsValue::from_str("kind"),
                                        ).ok()
                                            .and_then(|v| v.as_string())
                                            .unwrap_or_default();

                                        let item_type = js_sys::Reflect::get(
                                            &item_val,
                                            &wasm_bindgen::JsValue::from_str("type"),
                                        ).ok()
                                            .and_then(|v| v.as_string())
                                            .unwrap_or_default();

                                        if kind == "file" && item_type.starts_with("image/") {
                                            // Call getAsFile() on the DataTransferItem
                                            let get_as_file = js_sys::Reflect::get(
                                                &item_val,
                                                &wasm_bindgen::JsValue::from_str("getAsFile"),
                                            ).ok();

                                            if let Some(func) = get_as_file {
                                                if let Some(func) = func.dyn_ref::<js_sys::Function>() {
                                                    if let Ok(file_val) = func.call0(&item_val) {
                                                        if !file_val.is_null() && !file_val.is_undefined() {
                                                            let file: web_sys::File = file_val.unchecked_into();
                                                            ev.prevent_default();
                                                            read_file_as_base64(file, set_image_data);
                                                            return;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                let _ = ev;
            }
        }
    };

    // (USER-12) Clear all images (single + batch)
    let clear_all_images = move || {
        set_image_data.set(None);
        batch_images.set(Vec::new());
    };

    // Submit handler — now with auto-session creation (USER-22) and batch support (USER-12)
    let submit = move || {
        if sending.get() || is_model_missing() {
            return;
        }

        let text = input_text.get().trim().to_string();
        let img = image_data.get();
        let batch = batch_images.get();

        // Build the inference input
        let input = if batch.len() >= 2 {
            // (USER-12) Batch inference path
            let images = batch
                .iter()
                .map(|p| BatchImageInput {
                    data_base64: p.base64.clone(),
                    mime_type: p.mime_type.clone(),
                })
                .collect();
            InferenceInput::BatchImages {
                images,
                prompt: if text.is_empty() {
                    None
                } else {
                    Some(text.clone())
                },
            }
        } else if let Some(preview) = img {
            InferenceInput::Image {
                data_base64: preview.base64,
                mime_type: preview.mime_type,
                prompt: if text.is_empty() {
                    None
                } else {
                    Some(text.clone())
                },
            }
        } else if !text.is_empty() {
            InferenceInput::Text(text.clone())
        } else {
            return;
        };

        set_sending.set(true);
        set_input_text.set(String::new());
        clear_all_images();

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

        // Grab model_id now for potential session creation
        let model_id = active_model.get().map(|m| m.id.clone());

        leptos::task::spawn_local(async move {
            // (USER-22) Auto-create session if none exists
            let session_id = if let Some(id) = active_session_id.get() {
                id
            } else if let Some(mid) = model_id {
                match create_session(mid).await {
                    Ok(session) => {
                        let sid = session.id.clone();
                        set_active_session_id.set(Some(session.id));
                        sid
                    }
                    Err(e) => {
                        crate::components::toast::ToastStore::push(
                            format!("Failed to create session: {e}"),
                            crate::components::toast::ToastLevel::Error,
                        );
                        set_sending.set(false);
                        return;
                    }
                }
            } else {
                set_sending.set(false);
                return;
            };

            let _ = send_message(session_id, input).await;
            set_sending.set(false);
            // Bump version so ChatView re-fetches
            set_session_version.update(|v| *v += 1);
        });
    };

    // Handle keyboard in textarea
    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            submit();
        }
    };

    // Handle text input changes
    let on_input = move |ev: leptos::ev::Event| {
        let value = event_target_value(&ev);
        set_input_text.set(value);
        resize_textarea();
    };

    // Handle file selection from input — supports multiple selection
    let on_file_select = move |ev: leptos::ev::Event| {
        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                use wasm_bindgen::JsCast;
                let target = ev.target().unwrap();
                let input: web_sys::HtmlInputElement = target.unchecked_into();
                if let Some(files) = input.files() {
                    let count = files.length();
                    if count == 1 {
                        if let Some(file) = files.get(0) {
                            read_file_as_base64(file, set_image_data);
                        }
                    } else if count >= 2 {
                        // (USER-12) Multiple files selected — enter batch mode
                        for i in 0..count {
                            if let Some(file) = files.get(i) {
                                let mime = file.type_();
                                if mime.starts_with("image/") {
                                    read_file_as_base64_batch(file, batch_images);
                                }
                            }
                        }
                    }
                }
            } else {
                let _ = ev;
            }
        }
    };

    // Drag-and-drop handlers
    let on_dragover = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        set_drag_active.set(true);
    };

    let on_dragleave = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        set_drag_active.set(false);
    };

    // (USER-2, USER-12) Enhanced drop handler — single or batch
    let on_drop = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        set_drag_active.set(false);

        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                if let Some(dt) = ev.data_transfer() {
                    if let Some(files) = dt.files() {
                        let count = files.length();

                        // Validate all files are images
                        let mut has_non_image = false;
                        for i in 0..count {
                            if let Some(file) = files.get(i) {
                                if !file.type_().starts_with("image/") {
                                    has_non_image = true;
                                    break;
                                }
                            }
                        }

                        if has_non_image {
                            crate::components::toast::ToastStore::push(
                                "Only image files are supported".into(),
                                crate::components::toast::ToastLevel::Warning,
                            );
                            return;
                        }

                        if count == 1 {
                            // Single image — existing behavior
                            if let Some(file) = files.get(0) {
                                read_file_as_base64(file, set_image_data);
                            }
                        } else if count >= 2 {
                            // (USER-12) Multiple images — batch mode
                            // Clear any existing single image
                            set_image_data.set(None);
                            batch_images.set(Vec::new());
                            for i in 0..count {
                                if let Some(file) = files.get(i) {
                                    read_file_as_base64_batch(file, batch_images);
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    // Trigger hidden file input
    let open_file_picker = move |_| {
        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                if let Some(el) = file_input_ref.get() {
                    let el: &web_sys::HtmlInputElement = &el;
                    el.click();
                }
            }
        }
    };

    // Submit button click
    let on_submit_click = move |_: leptos::ev::MouseEvent| {
        submit();
    };

    // Outer container drag handlers so the drop zone appears
    // even when the model doesn't normally show the image zone
    let on_outer_dragover = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        set_drag_active.set(true);
    };
    let on_outer_dragleave = move |ev: leptos::ev::DragEvent| {
        cfg_if::cfg_if! {
            if #[cfg(feature = "hydrate")] {
                use wasm_bindgen::JsCast;
                // Only deactivate if the mouse actually left the container
                if let Some(container) = container_ref.get() {
                    let html: &web_sys::HtmlElement = container.unchecked_ref();
                    if let Some(related) = ev.related_target() {
                        if let Ok(node) = related.dyn_into::<web_sys::Node>() {
                            if html.contains(Some(&node)) {
                                return;
                            }
                        }
                    }
                }
            } else {
                let _ = &ev;
            }
        }
        set_drag_active.set(false);
    };
    let on_outer_drop = move |ev: leptos::ev::DragEvent| {
        ev.prevent_default();
        set_drag_active.set(false);
    };

    view! {
        <div
            node_ref=container_ref
            class="border-t border-zinc-800/60 bg-zinc-950/80 backdrop-blur-sm"
            on:paste=on_paste
            on:dragover=on_outer_dragover
            on:dragleave=on_outer_dragleave
            on:drop=on_outer_drop
        >
            <div class="max-w-3xl mx-auto px-4 py-3">
                // Image drop zone — shown for vision/multimodal models,
                // also shown during drag if model does NOT accept images (with warning)
                {move || {
                    let model_accepts_image = accepts_image();
                    let is_dragging = drag_active.get();

                    if !model_accepts_image && !is_dragging {
                        return view! { <div class="hidden"></div> }.into_any();
                    }

                    view! {
                        <div class="mb-2">
                            {move || {
                                let batch = batch_images.get();

                                if batch.len() >= 2 {
                                    // (USER-12) Batch preview — horizontal thumbnail strip
                                    let count = batch.len();
                                    let thumbs = batch
                                        .iter()
                                        .enumerate()
                                        .map(|(idx, preview)| {
                                            let url = preview.data_url.clone();
                                            view! {
                                                <div class="relative flex-shrink-0">
                                                    <img
                                                        src=url
                                                        alt=format!("Image {}", idx + 1)
                                                        class="h-16 w-16 rounded-lg object-cover border border-zinc-700/50"
                                                    />
                                                    <button
                                                        class="absolute -top-1 -right-1 w-4 h-4 bg-zinc-700 hover:bg-zinc-600 rounded-full flex items-center justify-center text-zinc-300 transition-colors"
                                                        on:click={
                                                            move |_| {
                                                                let mut imgs = batch_images.get();
                                                                if idx < imgs.len() {
                                                                    imgs.remove(idx);
                                                                }
                                                                // If only 1 remains, move it to single-image
                                                                if imgs.len() == 1 {
                                                                    let single = imgs.remove(0);
                                                                    set_image_data.set(Some(single));
                                                                    batch_images.set(Vec::new());
                                                                } else {
                                                                    batch_images.set(imgs);
                                                                }
                                                            }
                                                        }
                                                    >
                                                        <svg xmlns="http://www.w3.org/2000/svg" class="w-2.5 h-2.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                                                        </svg>
                                                    </button>
                                                </div>
                                            }
                                        })
                                        .collect_view();

                                    view! {
                                        <div class="relative">
                                            <div class="flex items-center gap-2 overflow-x-auto pb-1">
                                                {thumbs}
                                            </div>
                                            <div class="flex items-center justify-between mt-1.5">
                                                <span class="text-xs text-zinc-400 select-none">
                                                    {format!("{} images", count)}
                                                </span>
                                                <button
                                                    class="text-xs text-zinc-500 hover:text-zinc-300 transition-colors"
                                                    on:click=move |_| {
                                                        set_image_data.set(None);
                                                        batch_images.set(Vec::new());
                                                    }
                                                >
                                                    "Clear all"
                                                </button>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else if let Some(preview) = image_data.get() {
                                    // Show single thumbnail preview
                                    view! {
                                        <div class="relative inline-block">
                                            <img
                                                src=preview.data_url
                                                alt="Upload preview"
                                                class="h-20 rounded-lg object-cover border border-zinc-700/50"
                                            />
                                            <button
                                                class="absolute -top-1.5 -right-1.5 w-5 h-5 bg-zinc-700 hover:bg-zinc-600 rounded-full flex items-center justify-center text-zinc-300 transition-colors"
                                                on:click=move |_| set_image_data.set(None)
                                            >
                                                <svg xmlns="http://www.w3.org/2000/svg" class="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                                                    <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/>
                                                </svg>
                                            </button>
                                        </div>
                                    }.into_any()
                                } else {
                                    // Drop zone
                                    let model_ok = accepts_image();
                                    view! {
                                        <div
                                            class=move || format!(
                                                "drop-zone rounded-xl border-2 border-dashed p-4 text-center cursor-pointer transition-colors duration-150 {}",
                                                if drag_active.get() {
                                                    if model_ok {
                                                        "drop-zone-active border-amber-500/50 bg-amber-500/5"
                                                    } else {
                                                        "drop-zone-active border-red-500/50 bg-red-500/5"
                                                    }
                                                } else {
                                                    "border-zinc-700/40 hover:border-zinc-600/60 bg-zinc-800/20"
                                                }
                                            )
                                            on:dragover=on_dragover
                                            on:dragleave=on_dragleave
                                            on:drop=on_drop
                                            on:click=open_file_picker
                                        >
                                            <div class="flex flex-col items-center gap-1.5">
                                                {move || {
                                                    if drag_active.get() && !accepts_image() {
                                                        // (USER-2) Warning when model doesn't accept images
                                                        view! {
                                                            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 text-red-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                                                <path stroke-linecap="round" stroke-linejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126zM12 15.75h.007v.008H12v-.008z"/>
                                                            </svg>
                                                            <p class="text-xs text-red-400">"This model doesn't accept images"</p>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <svg xmlns="http://www.w3.org/2000/svg" class="w-5 h-5 text-zinc-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                                                                <path stroke-linecap="round" stroke-linejoin="round" d="M2.25 15.75l5.159-5.159a2.25 2.25 0 013.182 0l5.159 5.159m-1.5-1.5l1.409-1.41a2.25 2.25 0 013.182 0l2.909 2.91M3.75 21h16.5A2.25 2.25 0 0022.5 18.75V5.25A2.25 2.25 0 0020.25 3H3.75A2.25 2.25 0 001.5 5.25v13.5A2.25 2.25 0 003.75 21z"/>
                                                            </svg>
                                                            <p class="text-xs text-zinc-500">"Drop images or click to select"</p>
                                                        }.into_any()
                                                    }
                                                }}
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                            }}
                            // Hidden file input — supports multiple selection
                            <input
                                node_ref=file_input_ref
                                type="file"
                                accept="image/*"
                                multiple=true
                                class="hidden"
                                on:change=on_file_select
                            />
                        </div>
                    }.into_any()
                }}

                // Text input row
                <div class="flex items-end gap-2">
                    <textarea
                        node_ref=textarea_ref
                        class="flex-1 bg-zinc-800/40 border border-zinc-700/30 rounded-xl px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 resize-none focus:outline-none focus:ring-1 focus:ring-amber-500/30 focus:border-amber-500/30 transition-all duration-150 disabled:opacity-40 disabled:cursor-not-allowed"
                        style="height: 40px"
                        placeholder=placeholder_text
                        disabled=move || is_model_missing() || sending.get()
                        prop:value=move || input_text.get()
                        on:input=on_input
                        on:keydown=on_keydown
                        rows="1"
                    ></textarea>

                    // Send button
                    <button
                        class="flex-shrink-0 p-2.5 rounded-xl bg-amber-600 hover:bg-amber-500 disabled:bg-zinc-800 disabled:text-zinc-600 text-zinc-950 transition-colors duration-150 disabled:cursor-not-allowed"
                        disabled=move || {
                            is_model_missing() || sending.get() || (input_text.get().trim().is_empty() && !has_any_image())
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
    }
}

/// Preview data for an image that has been dropped or selected.
#[derive(Clone, Debug)]
struct ImagePreview {
    /// The base64-encoded image data (without the data URL prefix).
    base64: String,
    /// The MIME type of the image (e.g., "image/png").
    mime_type: String,
    /// A full data URL for previewing in an <img> tag.
    data_url: String,
}

/// Reads a `web_sys::File` as base64 using the FileReader API.
/// Sets the `ImagePreview` signal when done (single-image mode).
#[cfg(feature = "hydrate")]
fn read_file_as_base64(file: web_sys::File, set_image_data: WriteSignal<Option<ImagePreview>>) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let mime_type = file.type_();
    let reader = web_sys::FileReader::new().expect("FileReader");
    reader.read_as_data_url(&file).ok();

    let onload = Closure::wrap(Box::new(move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let reader: web_sys::FileReader = target.unchecked_into();
        if let Ok(result) = reader.result() {
            if let Some(data_url) = result.as_string() {
                // data_url looks like "data:image/png;base64,iVBOR..."
                let base64 = data_url.split(',').nth(1).unwrap_or("").to_string();
                set_image_data.set(Some(ImagePreview {
                    base64,
                    mime_type: mime_type.clone(),
                    data_url: data_url.clone(),
                }));
            }
        }
    }) as Box<dyn FnMut(_)>);

    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    // Prevent the closure from being dropped while the read is in progress
    onload.forget();
}

/// (USER-12) Reads a `web_sys::File` as base64 and appends it to the batch images signal.
#[cfg(feature = "hydrate")]
fn read_file_as_base64_batch(file: web_sys::File, batch_images: RwSignal<Vec<ImagePreview>>) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let mime_type = file.type_();
    let reader = web_sys::FileReader::new().expect("FileReader");
    reader.read_as_data_url(&file).ok();

    let onload = Closure::wrap(Box::new(move |ev: web_sys::Event| {
        let target = ev.target().unwrap();
        let reader: web_sys::FileReader = target.unchecked_into();
        if let Ok(result) = reader.result() {
            if let Some(data_url) = result.as_string() {
                let base64 = data_url.split(',').nth(1).unwrap_or("").to_string();
                batch_images.update(|imgs| {
                    imgs.push(ImagePreview {
                        base64,
                        mime_type: mime_type.clone(),
                        data_url: data_url.clone(),
                    });
                });
            }
        }
    }) as Box<dyn FnMut(_)>);

    reader.set_onload(Some(onload.as_ref().unchecked_ref()));
    onload.forget();
}

// No SSR stub needed — calls to `read_file_as_base64` and `read_file_as_base64_batch`
// are gated behind `cfg_if! { if #[cfg(feature = "hydrate")] { ... } }` in the component body.
