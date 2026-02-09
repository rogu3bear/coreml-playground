/// Session export utilities: Markdown, HTML, and JSON rendering.
///
/// The pure rendering functions (`export_as_markdown`, `export_as_html`) work
/// on both SSR and WASM targets. The `ExportMenu` Leptos component and its
/// clipboard / download helpers are client-side only, gated behind
/// `cfg(feature = "hydrate")`.
use crate::types::{ChatMessage, MessageContent, MessageRole};

// ---------------------------------------------------------------------------
// Pure export renderers (no platform deps, testable everywhere)
// ---------------------------------------------------------------------------

/// Renders a chat session as Markdown.
///
/// User messages appear as **User**: ..., model messages as **Model**: ...,
/// with inference timing shown in a blockquote. Images are represented as
/// `[Image: mime_type]` and batches as `[Batch: N images]`.
pub fn export_as_markdown(model_name: &str, messages: &[ChatMessage]) -> String {
    let mut out = format!("# Chat Session \u{2014} {model_name}\n");

    for msg in messages {
        out.push('\n');
        let role_label = match msg.role {
            MessageRole::User => "User",
            MessageRole::Model => "Model",
            MessageRole::System => "System",
        };

        let body = content_to_markdown(&msg.content);
        out.push_str(&format!("**{role_label}**: {body}\n"));

        if let Some(ms) = msg.inference_ms {
            out.push_str(&format!("\n> Inference time: {ms}ms\n"));
        }

        out.push_str("\n---\n");
    }

    out
}

/// Renders a chat session as a self-contained HTML document with inline CSS.
///
/// The page uses a zinc/amber color scheme that mirrors the app's dark theme.
/// All styles are inlined so no external dependencies are required.
pub fn export_as_html(model_name: &str, messages: &[ChatMessage]) -> String {
    let mut body = String::new();

    for msg in messages {
        let (role_label, role_class) = match msg.role {
            MessageRole::User => ("User", "user"),
            MessageRole::Model => ("Model", "model"),
            MessageRole::System => ("System", "system"),
        };

        let content_html = content_to_html(&msg.content);
        let timing = msg
            .inference_ms
            .map(|ms| format!(r#"<div class="timing">Inference time: {ms}ms</div>"#))
            .unwrap_or_default();

        body.push_str(&format!(
            r#"<div class="message {role_class}">
  <div class="role">{role_label}</div>
  <div class="content">{content_html}</div>
  {timing}
</div>
"#
        ));
    }

    let escaped_title = html_escape(model_name);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Chat Session — {escaped_title}</title>
<style>
  :root {{
    --bg: #18181b;
    --surface: #27272a;
    --surface-alt: #3f3f46;
    --text: #fafafa;
    --text-muted: #a1a1aa;
    --accent: #f59e0b;
    --user-bg: #1e3a5f;
    --model-bg: #292524;
    --border: #3f3f46;
  }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: var(--bg);
    color: var(--text);
    line-height: 1.6;
    padding: 2rem;
    max-width: 800px;
    margin: 0 auto;
  }}
  h1 {{
    color: var(--accent);
    margin-bottom: 1.5rem;
    font-size: 1.5rem;
    border-bottom: 1px solid var(--border);
    padding-bottom: 0.75rem;
  }}
  .message {{
    margin-bottom: 1rem;
    padding: 1rem;
    border-radius: 8px;
    border: 1px solid var(--border);
  }}
  .message.user {{
    background: var(--user-bg);
  }}
  .message.model {{
    background: var(--model-bg);
  }}
  .message.system {{
    background: var(--surface);
    opacity: 0.7;
  }}
  .role {{
    font-weight: 600;
    color: var(--accent);
    margin-bottom: 0.25rem;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }}
  .content {{
    white-space: pre-wrap;
    word-wrap: break-word;
  }}
  .content pre {{
    background: var(--surface-alt);
    padding: 0.75rem;
    border-radius: 4px;
    overflow-x: auto;
    margin: 0.5rem 0;
  }}
  .timing {{
    margin-top: 0.5rem;
    font-size: 0.8rem;
    color: var(--text-muted);
    font-style: italic;
  }}
  .image-placeholder {{
    display: inline-block;
    background: var(--surface-alt);
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    font-size: 0.85rem;
    color: var(--text-muted);
  }}
</style>
</head>
<body>
<h1>Chat Session &mdash; {escaped_title}</h1>
{body}
</body>
</html>"#
    )
}

// ---------------------------------------------------------------------------
// Content formatting helpers
// ---------------------------------------------------------------------------

fn content_to_markdown(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(s) => s.clone(),
        MessageContent::Image { mime_type, caption, .. } => {
            let cap = caption
                .as_deref()
                .map(|c| format!(" \u{2014} {c}"))
                .unwrap_or_default();
            format!("[Image: {mime_type}]{cap}")
        }
        MessageContent::ModelOutput(val) => {
            serde_json::to_string_pretty(val).unwrap_or_else(|_| val.to_string())
        }
        MessageContent::Streaming { partial, .. } => partial.clone(),
        MessageContent::Batch(items) => format!("[Batch: {} images]", items.len()),
    }
}

fn content_to_html(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(s) => html_escape(s),
        MessageContent::Image { mime_type, caption, .. } => {
            let cap = caption
                .as_deref()
                .map(|c| format!(" &mdash; {}", html_escape(c)))
                .unwrap_or_default();
            format!(
                r#"<span class="image-placeholder">[Image: {}]{}</span>"#,
                html_escape(mime_type),
                cap,
            )
        }
        MessageContent::ModelOutput(val) => {
            let json = serde_json::to_string_pretty(val).unwrap_or_else(|_| val.to_string());
            format!("<pre>{}</pre>", html_escape(&json))
        }
        MessageContent::Streaming { partial, .. } => html_escape(partial),
        MessageContent::Batch(items) => {
            format!(
                r#"<span class="image-placeholder">[Batch: {} images]</span>"#,
                items.len()
            )
        }
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// ExportMenu component (hydrate-only clipboard / download helpers)
// ---------------------------------------------------------------------------

use leptos::prelude::*;

/// A dropdown menu offering Markdown, HTML, and JSON export for a session.
///
/// The component fetches session data via the `ExportSession` server function,
/// then uses browser APIs (clipboard, Blob URL) to deliver the output.
#[component]
pub fn ExportMenu(
    /// The session ID to export.
    session_id: Signal<String>,
    /// Controls visibility of the dropdown.
    #[prop(into)]
    show: Signal<bool>,
    /// Called when the menu should close.
    on_close: Callback<()>,
) -> impl IntoView {
    use crate::server::api::export_session;

    let copy_markdown = move |_| {
        let sid = session_id.get();
        let on_close = on_close.clone();
        leptos::task::spawn_local(async move {
            match export_session(sid).await {
                Ok((model_name, messages)) => {
                    let md = export_as_markdown(&model_name, &messages);
                    do_copy_to_clipboard(&md);
                }
                Err(e) => {
                    leptos::logging::log!("[export] error: {e}");
                }
            }
            on_close.run(());
        });
    };

    let copy_json = move |_| {
        let sid = session_id.get();
        let on_close = on_close.clone();
        leptos::task::spawn_local(async move {
            match export_session(sid).await {
                Ok((_model_name, messages)) => {
                    let json = serde_json::to_string_pretty(&messages).unwrap_or_default();
                    do_copy_to_clipboard(&json);
                }
                Err(e) => {
                    leptos::logging::log!("[export] error: {e}");
                }
            }
            on_close.run(());
        });
    };

    let download_html = move |_| {
        let sid = session_id.get();
        let on_close = on_close.clone();
        leptos::task::spawn_local(async move {
            match export_session(sid).await {
                Ok((model_name, messages)) => {
                    let html = export_as_html(&model_name, &messages);
                    do_download_file("session.html", "text/html", &html);
                }
                Err(e) => {
                    leptos::logging::log!("[export] error: {e}");
                }
            }
            on_close.run(());
        });
    };

    view! {
        <Show when=move || show.get()>
            <div class="absolute right-0 top-full mt-1 z-50 w-48 rounded-lg border border-zinc-700 bg-zinc-800 shadow-xl py-1 text-sm">
                <button
                    class="w-full text-left px-3 py-2 hover:bg-zinc-700 text-zinc-200 flex items-center gap-2"
                    on:click=copy_markdown
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 text-zinc-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M9 12h6m-3-3v6m-7 4h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/>
                    </svg>
                    "Copy as Markdown"
                </button>
                <button
                    class="w-full text-left px-3 py-2 hover:bg-zinc-700 text-zinc-200 flex items-center gap-2"
                    on:click=download_html
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 text-zinc-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M4 16v2a2 2 0 002 2h12a2 2 0 002-2v-2m-4-4l-4 4m0 0l-4-4m4 4V4"/>
                    </svg>
                    "Download as HTML"
                </button>
                <button
                    class="w-full text-left px-3 py-2 hover:bg-zinc-700 text-zinc-200 flex items-center gap-2"
                    on:click=copy_json
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4 text-zinc-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                        <path stroke-linecap="round" stroke-linejoin="round" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"/>
                    </svg>
                    "Copy as JSON"
                </button>
            </div>
        </Show>
    }
}

// ---------------------------------------------------------------------------
// Browser helpers — clipboard and file download
// ---------------------------------------------------------------------------

/// Copy text to the clipboard. No-op on SSR.
fn do_copy_to_clipboard(text: &str) {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            if let Some(window) = web_sys::window() {
                if let Ok(navigator) = js_sys::Reflect::get(&window, &"navigator".into()) {
                    if let Ok(clipboard) = js_sys::Reflect::get(&navigator, &"clipboard".into()) {
                        let _ = js_sys::Reflect::apply(
                            &js_sys::Function::from(
                                js_sys::Reflect::get(&clipboard, &"writeText".into())
                                    .unwrap_or(wasm_bindgen::JsValue::UNDEFINED),
                            ),
                            &clipboard,
                            &js_sys::Array::of1(&wasm_bindgen::JsValue::from_str(text)),
                        );
                    }
                }
            }
        } else {
            let _ = text;
        }
    }
}

/// Trigger a file download in the browser by creating a temporary Blob URL
/// and clicking a hidden `<a>` element. No-op on SSR.
fn do_download_file(filename: &str, mime_type: &str, content: &str) {
    cfg_if::cfg_if! {
        if #[cfg(feature = "hydrate")] {
            use wasm_bindgen::JsCast;

            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let document = match window.document() {
                Some(d) => d,
                None => return,
            };

            // Build a Blob from the content string.
            let parts = js_sys::Array::new();
            parts.push(&wasm_bindgen::JsValue::from_str(content));

            let opts = web_sys::BlobPropertyBag::new();
            opts.set_type(mime_type);

            let blob = match web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts) {
                Ok(b) => b,
                Err(_) => return,
            };

            let url = match web_sys::Url::create_object_url_with_blob(&blob) {
                Ok(u) => u,
                Err(_) => return,
            };

            // Create an invisible anchor, click it, then clean up.
            if let Ok(el) = document.create_element("a") {
                if let Some(anchor) = el.dyn_ref::<web_sys::HtmlElement>() {
                    let _ = anchor.set_attribute("href", &url);
                    let _ = anchor.set_attribute("download", filename);
                    anchor.style().set_property("display", "none").ok();

                    if let Some(body) = document.body() {
                        let _ = body.append_child(anchor);
                        anchor.click();
                        let _ = body.remove_child(anchor);
                    }
                }
                let _ = web_sys::Url::revoke_object_url(&url);
            }
        } else {
            let _ = (filename, mime_type, content);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn sample_messages() -> Vec<ChatMessage> {
        vec![
            ChatMessage {
                id: "1".into(),
                role: MessageRole::User,
                content: MessageContent::Text("Hello world".into()),
                timestamp: 1000,
                inference_ms: None,
            },
            ChatMessage {
                id: "2".into(),
                role: MessageRole::Model,
                content: MessageContent::ModelOutput(serde_json::json!({"label": "Positive", "score": 0.95})),
                timestamp: 1001,
                inference_ms: Some(127),
            },
            ChatMessage {
                id: "3".into(),
                role: MessageRole::User,
                content: MessageContent::Image {
                    data_base64: "abc123".into(),
                    mime_type: "image/png".into(),
                    caption: Some("A photo".into()),
                },
                timestamp: 1002,
                inference_ms: None,
            },
            ChatMessage {
                id: "4".into(),
                role: MessageRole::Model,
                content: MessageContent::Batch(vec![
                    BatchItem {
                        image_base64: "img1".into(),
                        mime_type: "image/jpeg".into(),
                        output: serde_json::json!({"class": "cat"}),
                        inference_ms: Some(50),
                    },
                    BatchItem {
                        image_base64: "img2".into(),
                        mime_type: "image/jpeg".into(),
                        output: serde_json::json!({"class": "dog"}),
                        inference_ms: Some(48),
                    },
                ]),
                timestamp: 1003,
                inference_ms: Some(98),
            },
        ]
    }

    #[test]
    fn markdown_contains_model_name() {
        let md = export_as_markdown("TestModel", &sample_messages());
        assert!(md.starts_with("# Chat Session \u{2014} TestModel\n"));
    }

    #[test]
    fn markdown_renders_all_roles() {
        let md = export_as_markdown("M", &sample_messages());
        assert!(md.contains("**User**: Hello world"));
        assert!(md.contains("**Model**:"));
        assert!(md.contains("> Inference time: 127ms"));
    }

    #[test]
    fn markdown_image_placeholder() {
        let md = export_as_markdown("M", &sample_messages());
        assert!(md.contains("[Image: image/png]"));
    }

    #[test]
    fn markdown_batch_placeholder() {
        let md = export_as_markdown("M", &sample_messages());
        assert!(md.contains("[Batch: 2 images]"));
    }

    #[test]
    fn html_is_self_contained() {
        let html = export_as_html("TestModel", &sample_messages());
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("<style>"));
        assert!(html.contains("Chat Session"));
        assert!(html.contains("TestModel"));
        // No external stylesheet links
        assert!(!html.contains("<link rel=\"stylesheet\""));
    }

    #[test]
    fn html_escapes_special_characters() {
        let msgs = vec![ChatMessage {
            id: "x".into(),
            role: MessageRole::User,
            content: MessageContent::Text("<script>alert('xss')</script>".into()),
            timestamp: 0,
            inference_ms: None,
        }];
        let html = export_as_html("M", &msgs);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
