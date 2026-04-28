/// WebSocket-based inference handler and supporting logic.
///
/// The main entry point is `ws_handler`, which upgrades an HTTP request to a
/// WebSocket connection. The client sends `InferenceRequest` messages as JSON;
/// the server runs inference via the CoreML bridge and streams the result back
/// as a series of `WsMessage` frames.
///
/// Because CoreML predictions are synchronous (no true token-by-token
/// streaming), the current implementation sends the full output as a single
/// `WsMessage::Token` followed by `WsMessage::Done`. When streaming support
/// is added to the bridge the inner loop can be adjusted without changing the
/// wire protocol.
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension,
    },
    response::IntoResponse,
};
use serde_json;

use crate::types::*;

use super::interpreter::interpret_output;
use super::model_registry::ModelRegistry;
use super::session_store::SessionStore;

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

/// Axum handler that upgrades the connection to a WebSocket for inference
/// streaming.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(registry): Extension<Arc<ModelRegistry>>,
    Extension(store): Extension<Arc<SessionStore>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, registry, store))
}

/// Process messages on an established WebSocket connection.
///
/// Each incoming text frame is expected to be a JSON-encoded
/// `InferenceRequest`. The handler:
///
/// 1. Persists the user message in the session store.
/// 2. Runs inference (or a mock prediction if the bridge is unavailable).
/// 3. Streams the result back as `WsMessage::Token` + `WsMessage::Done`.
/// 4. Persists the model's response message.
async fn handle_socket(
    mut socket: WebSocket,
    registry: Arc<ModelRegistry>,
    store: Arc<SessionStore>,
) {
    while let Some(Ok(frame)) = socket.recv().await {
        let text = match frame {
            Message::Text(t) => t.to_string(),
            Message::Close(_) => break,
            // Ignore binary / ping / pong frames.
            _ => continue,
        };

        let request: InferenceRequest = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(err) => {
                let _ = send_ws(
                    &mut socket,
                    &WsMessage::Error(format!("invalid request: {err}")),
                )
                .await;
                continue;
            }
        };

        // Persist the user's message.
        let user_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: input_to_content(&request.input),
            timestamp: chrono::Utc::now().timestamp(),
            inference_ms: None,
        };

        if let Err(err) = persist_message(&store, &request.session_id, &user_msg).await {
            leptos::logging::log!("[ws] failed to persist user message: {err}");
        }

        // Ensure the requested model is active.
        let active_id = registry.get_active_id().await;
        if active_id.as_deref() != Some(&request.model_id) {
            if let Err(err) = registry.load_model(&request.model_id).await {
                let _ = send_ws(
                    &mut socket,
                    &WsMessage::Error(format!("failed to load model: {err}")),
                )
                .await;
                continue;
            }
        }

        // Run inference.
        let start = std::time::Instant::now();
        let result = run_inference(&registry, &request.input).await;
        let inference_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                // Get model info for the interpreter so it has context about
                // model_type and output_schema.
                let model_info = registry.get_model_info(&request.model_id).await;
                let (model_type, output_schema) = model_info
                    .map(|mi| (mi.model_type, mi.output_schema))
                    .unwrap_or((ModelType::Unknown, vec![]));

                let formatted = interpret_output(&output, &model_type, &output_schema);
                let token_text = if let Some(detail) = &formatted.detail {
                    format!("{}\n\n{}", formatted.summary, detail)
                } else {
                    formatted.summary.clone()
                };

                let _ = send_ws(&mut socket, &WsMessage::Token(token_text.clone())).await;
                let _ = send_ws(&mut socket, &WsMessage::Done { inference_ms }).await;

                // Persist the model response.
                let model_msg = ChatMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    role: MessageRole::Model,
                    content: MessageContent::ModelOutput(output),
                    timestamp: chrono::Utc::now().timestamp(),
                    inference_ms: Some(inference_ms),
                };
                if let Err(err) = persist_message(&store, &request.session_id, &model_msg).await {
                    leptos::logging::log!("[ws] failed to persist model message: {err}");
                }
            }
            Err(err) => {
                let _ = send_ws(&mut socket, &WsMessage::Error(err)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Inference execution
// ---------------------------------------------------------------------------

/// Public entry point for the `api.rs` server functions to run inference
/// without going through a WebSocket.
pub async fn run_inference_for_api(
    registry: &Arc<ModelRegistry>,
    input: &InferenceInput,
) -> Result<serde_json::Value, String> {
    run_inference(registry, input).await
}

/// Run inference using the currently loaded model.
///
/// When the real CoreML bridge is not available this returns mock output that
/// mirrors what a real model would produce, keyed on the model type.
async fn run_inference(
    registry: &Arc<ModelRegistry>,
    input: &InferenceInput,
) -> Result<serde_json::Value, String> {
    let handle = registry
        .get_active()
        .await
        .ok_or_else(|| "no model loaded".to_string())?;

    // CPO-9: Demo echo model — return the input with a note, bypassing CoreML.
    if handle.id == "demo-echo" {
        return demo_echo_predict(input).await;
    }

    // TODO: Replace with real bridge calls once ffi.rs is ready:
    //
    //   match input {
    //       InferenceInput::Text(text) =>
    //           bridge::ffi::predict_text(&handle, text),
    //       InferenceInput::Image { data_base64, prompt, .. } => {
    //           let bytes = base64_decode(data_base64)?;
    //           bridge::ffi::predict_image(&handle, &bytes, prompt.as_deref())
    //       }
    //   }

    // For now, return mock predictions.
    mock_predict(&handle.id, input).await
}

/// Demo echo model inference (CPO-9).
///
/// Returns the user's input back as JSON with a helpful note. Gives new users
/// a zero-friction first experience without requiring real CoreML models.
async fn demo_echo_predict(input: &InferenceInput) -> Result<serde_json::Value, String> {
    // Small delay so the UI flow feels natural.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let echo_text = match input {
        InferenceInput::Text(t) => t.clone(),
        InferenceInput::Image {
            prompt, mime_type, ..
        } => prompt
            .clone()
            .unwrap_or_else(|| format!("[image: {mime_type}]")),
        InferenceInput::BatchImages { images, prompt } => prompt
            .clone()
            .unwrap_or_else(|| format!("[batch: {} images]", images.len())),
    };

    Ok(serde_json::json!({
        "echo": echo_text,
        "note": "This is a demo model. Load a real CoreML model to get started."
    }))
}

/// Generate a mock prediction based on the model id and input type.
async fn mock_predict(model_id: &str, input: &InferenceInput) -> Result<serde_json::Value, String> {
    // Simulate a short latency so the UI feels realistic.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    match model_id {
        "mock-mobilenetv2" => Ok(serde_json::json!({
            "classLabel": "golden retriever",
            "classLabelProbs": {
                "golden retriever": 0.87,
                "Labrador retriever": 0.08,
                "cocker spaniel": 0.03,
            }
        })),
        "mock-textsentiment" => {
            let text = match input {
                InferenceInput::Text(t) => t.as_str(),
                InferenceInput::Image {
                    prompt: Some(p), ..
                } => p.as_str(),
                InferenceInput::BatchImages {
                    prompt: Some(p), ..
                } => p.as_str(),
                _ => "",
            };
            let positive = text.contains("great")
                || text.contains("love")
                || text.contains("good")
                || text.contains("happy")
                || text.contains("excellent");
            Ok(serde_json::json!({
                "label": if positive { "Positive" } else { "Negative" },
                "score": if positive { 0.92 } else { 0.78 },
            }))
        }
        "mock-whispertiny" => Ok(serde_json::json!({
            "text": "This is a mock transcription of the provided audio clip."
        })),
        _ => Ok(serde_json::json!({
            "output_0": [0.5, 0.3, 0.2],
            "debug": { "model": model_id, "mock": true }
        })),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Send a `WsMessage` as a JSON text frame.
async fn send_ws(socket: &mut WebSocket, msg: &WsMessage) -> Result<(), String> {
    let json = serde_json::to_string(msg).map_err(|e| format!("serialize error: {e}"))?;
    socket
        .send(Message::Text(json.into()))
        .await
        .map_err(|e| format!("send error: {e}"))
}

/// Convert an `InferenceInput` into a `MessageContent` for persistence.
pub(crate) fn input_to_content(input: &InferenceInput) -> MessageContent {
    match input {
        InferenceInput::Text(text) => MessageContent::Text(text.clone()),
        InferenceInput::Image {
            data_base64,
            mime_type,
            prompt,
        } => MessageContent::Image {
            data_base64: data_base64.clone(),
            mime_type: mime_type.clone(),
            caption: prompt.clone(),
        },
        InferenceInput::BatchImages { images, prompt } => {
            if let Some(first) = images.first() {
                MessageContent::Image {
                    data_base64: first.data_base64.clone(),
                    mime_type: first.mime_type.clone(),
                    caption: Some(prompt.clone().unwrap_or_else(|| {
                        let n = images.len();
                        if n == 1 {
                            "Batch: 1 image".to_string()
                        } else {
                            format!("Batch: {} images", n)
                        }
                    })),
                }
            } else {
                MessageContent::Text(prompt.clone().unwrap_or_else(|| "Empty batch".to_string()))
            }
        }
    }
}

/// Persist a chat message, running the blocking SQLite call on a dedicated
/// thread so we do not block the Tokio runtime.
async fn persist_message(
    store: &Arc<SessionStore>,
    session_id: &str,
    msg: &ChatMessage,
) -> Result<(), String> {
    let store = store.clone();
    let session_id = session_id.to_string();
    let msg = msg.clone();
    tokio::task::spawn_blocking(move || store.add_message(&session_id, &msg))
        .await
        .map_err(|e| format!("spawn_blocking error: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_to_content_text() {
        let input = InferenceInput::Text("hello".into());
        let content = input_to_content(&input);
        assert_eq!(content.as_text(), Some("hello"));
    }

    #[test]
    fn input_to_content_image() {
        let input = InferenceInput::Image {
            data_base64: "abc".into(),
            mime_type: "image/png".into(),
            prompt: Some("describe this".into()),
        };
        let content = input_to_content(&input);

        if let MessageContent::Image {
            data_base64,
            mime_type,
            caption,
        } = content
        {
            assert_eq!(data_base64, "abc");
            assert_eq!(mime_type, "image/png");
            assert_eq!(caption.as_deref(), Some("describe this"));
        } else {
            panic!("expected Image content, got {:?}", content);
        }
    }

    #[test]
    fn input_to_content_batch_nonempty() {
        let input = InferenceInput::BatchImages {
            images: vec![
                BatchImageInput {
                    data_base64: "img1".into(),
                    mime_type: "image/jpeg".into(),
                },
                BatchImageInput {
                    data_base64: "img2".into(),
                    mime_type: "image/png".into(),
                },
            ],
            prompt: Some("batch prompt".into()),
        };
        let content = input_to_content(&input);

        if let MessageContent::Image {
            data_base64,
            caption,
            ..
        } = content
        {
            assert_eq!(data_base64, "img1", "should use first image's data");
            assert_eq!(caption.as_deref(), Some("batch prompt"));
        } else {
            panic!(
                "expected Image content for nonempty batch, got {:?}",
                content
            );
        }
    }

    #[test]
    fn input_to_content_batch_single_image_grammar() {
        let input = InferenceInput::BatchImages {
            images: vec![BatchImageInput {
                data_base64: "img1".into(),
                mime_type: "image/jpeg".into(),
            }],
            prompt: None,
        };
        let content = input_to_content(&input);
        if let MessageContent::Image { caption, .. } = content {
            let cap = caption.unwrap();
            assert!(cap.contains("1 image"), "got: {}", cap);
            assert!(!cap.contains("1 images"), "got: {}", cap);
        } else {
            panic!("expected Image content");
        }
    }

    #[test]
    fn input_to_content_batch_empty() {
        let input = InferenceInput::BatchImages {
            images: vec![],
            prompt: None,
        };
        let content = input_to_content(&input);
        assert_eq!(
            content.as_text(),
            Some("Empty batch"),
            "empty batch with no prompt should produce 'Empty batch' text"
        );
    }
}
