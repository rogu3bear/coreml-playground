/// Leptos server functions for the CoreML Playground API.
///
/// Each function is registered via the `#[server]` macro so Leptos can
/// generate corresponding client-side RPC stubs. On the server they extract
/// `ModelRegistry` and `SessionStore` from the Axum `Extension` layer and
/// delegate to the appropriate service methods.
///
/// All SQLite operations go through `tokio::task::spawn_blocking` because
/// `rusqlite::Connection` is not `Send` across `.await` points.
use leptos::prelude::*;
use crate::types::*;

// SSR-only imports — the `#[server]` macro generates cfg-gated code, but
// the body itself also needs access to server-side types.
#[cfg(feature = "ssr")]
use {
    std::sync::Arc,
    axum::extract::Extension,
    super::model_registry::ModelRegistry,
    super::session_store::SessionStore,
};

// ---------------------------------------------------------------------------
// Model queries
// ---------------------------------------------------------------------------

/// Return metadata for every model discovered in the models directory.
#[server(ListModels)]
pub async fn list_models() -> Result<Vec<ModelInfo>, ServerFnError> {
    let Extension(registry) =
        leptos_axum::extract::<Extension<Arc<ModelRegistry>>>().await?;
    Ok(registry.list_models().await)
}

/// Load a model into memory so it can be used for inference.
///
/// Only one model can be active at a time; loading a new model automatically
/// unloads the previous one.
#[server(LoadModel)]
pub async fn load_model(model_id: String) -> Result<ModelInfo, ServerFnError> {
    let Extension(registry) =
        leptos_axum::extract::<Extension<Arc<ModelRegistry>>>().await?;
    registry
        .load_model(&model_id)
        .await
        .map_err(|e| ServerFnError::new(e))
}

// ---------------------------------------------------------------------------
// Session management
// ---------------------------------------------------------------------------

/// List all chat sessions, ordered by most-recently-updated first.
#[server(ListSessions)]
pub async fn list_sessions() -> Result<Vec<Session>, ServerFnError> {
    let Extension(store) =
        leptos_axum::extract::<Extension<Arc<SessionStore>>>().await?;

    tokio::task::spawn_blocking(move || store.list_sessions())
        .await
        .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
        .map_err(|e| ServerFnError::new(e))
}

/// Retrieve all messages for a given session.
#[server(GetSessionMessages)]
pub async fn get_session_messages(
    session_id: String,
) -> Result<Vec<ChatMessage>, ServerFnError> {
    let Extension(store) =
        leptos_axum::extract::<Extension<Arc<SessionStore>>>().await?;

    tokio::task::spawn_blocking(move || store.get_messages(&session_id))
        .await
        .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
        .map_err(|e| ServerFnError::new(e))
}

/// Create a new chat session for a given model.
///
/// The model does not need to be loaded yet — the session merely records
/// which model it is associated with. The model name is resolved from the
/// registry so the session stores a human-readable label.
#[server(CreateSession)]
pub async fn create_session(model_id: String) -> Result<Session, ServerFnError> {
    let Extension(registry) =
        leptos_axum::extract::<Extension<Arc<ModelRegistry>>>().await?;
    let Extension(store) =
        leptos_axum::extract::<Extension<Arc<SessionStore>>>().await?;

    let model_name = registry
        .get_model_info(&model_id)
        .await
        .map(|info| info.name)
        .unwrap_or_else(|| "Unknown Model".to_string());

    tokio::task::spawn_blocking(move || store.create_session(&model_id, &model_name))
        .await
        .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
        .map_err(|e| ServerFnError::new(e))
}

/// Rename a session (update its display name).
#[server(RenameSession)]
pub async fn rename_session(
    session_id: String,
    new_name: String,
) -> Result<(), ServerFnError> {
    let Extension(store) =
        leptos_axum::extract::<Extension<Arc<SessionStore>>>().await?;

    tokio::task::spawn_blocking(move || store.rename_session(&session_id, &new_name))
        .await
        .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
        .map_err(|e| ServerFnError::new(e))
}

/// Delete a session and all of its messages.
#[server(DeleteSession)]
pub async fn delete_session(session_id: String) -> Result<(), ServerFnError> {
    let Extension(store) =
        leptos_axum::extract::<Extension<Arc<SessionStore>>>().await?;

    tokio::task::spawn_blocking(move || store.delete_session(&session_id))
        .await
        .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
        .map_err(|e| ServerFnError::new(e))
}

// ---------------------------------------------------------------------------
// Inference (non-streaming)
// ---------------------------------------------------------------------------

/// Send a message to the active model and receive the response.
///
/// This is the non-streaming counterpart to the WebSocket handler. It:
/// 1. Persists the user message.
/// 2. Ensures the correct model is loaded.
/// 3. Runs inference (mock or real).
/// 4. Persists and returns the model's response.
///
/// For streaming results, use the `/ws/inference` WebSocket endpoint instead.
#[server(SendChatMessage)]
pub async fn send_message(
    session_id: String,
    input: InferenceInput,
) -> Result<ChatMessage, ServerFnError> {
    let Extension(registry) =
        leptos_axum::extract::<Extension<Arc<ModelRegistry>>>().await?;
    let Extension(store) =
        leptos_axum::extract::<Extension<Arc<SessionStore>>>().await?;

    // Look up the session to find its associated model.
    let session = {
        let store = store.clone();
        let sid = session_id.clone();
        tokio::task::spawn_blocking(move || store.get_session(&sid))
            .await
            .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
            .map_err(|e| ServerFnError::new(e))?
            .ok_or_else(|| ServerFnError::new(format!("session not found: {session_id}")))?
    };

    // Persist the user message.
    let user_msg = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: MessageRole::User,
        content: input_to_content(&input),
        timestamp: chrono::Utc::now().timestamp(),
        inference_ms: None,
    };

    {
        let store = store.clone();
        let sid = session_id.clone();
        let msg = user_msg.clone();
        tokio::task::spawn_blocking(move || store.add_message(&sid, &msg))
            .await
            .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
            .map_err(|e| ServerFnError::new(e))?;
    }

    // Ensure the model is loaded.
    let active_id = registry.get_active_id().await;
    if active_id.as_deref() != Some(&session.model_id) {
        registry
            .load_model(&session.model_id)
            .await
            .map_err(|e| ServerFnError::new(e))?;
    }

    // Run inference — batch or single.
    let model_msg = match &input {
        InferenceInput::BatchImages { images, .. } => {
            let mut items = Vec::with_capacity(images.len());
            let total_start = std::time::Instant::now();

            for img in images {
                let single_input = InferenceInput::Image {
                    data_base64: img.data_base64.clone(),
                    mime_type: img.mime_type.clone(),
                    prompt: None,
                };
                let start = std::time::Instant::now();
                let output = super::inference::run_inference_for_api(&registry, &single_input)
                    .await
                    .map_err(|e| ServerFnError::new(e))?;
                let ms = start.elapsed().as_millis() as u64;

                items.push(BatchItem {
                    image_base64: img.data_base64.clone(),
                    mime_type: img.mime_type.clone(),
                    output,
                    inference_ms: Some(ms),
                });
            }

            let total_ms = total_start.elapsed().as_millis() as u64;
            ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: MessageRole::Model,
                content: MessageContent::Batch(items),
                timestamp: chrono::Utc::now().timestamp(),
                inference_ms: Some(total_ms),
            }
        }
        _ => {
            let start = std::time::Instant::now();
            let output = super::inference::run_inference_for_api(&registry, &input)
                .await
                .map_err(|e| ServerFnError::new(e))?;
            let inference_ms = start.elapsed().as_millis() as u64;

            ChatMessage {
                id: uuid::Uuid::new_v4().to_string(),
                role: MessageRole::Model,
                content: MessageContent::ModelOutput(output),
                timestamp: chrono::Utc::now().timestamp(),
                inference_ms: Some(inference_ms),
            }
        }
    };

    {
        let store = store.clone();
        let sid = session_id.clone();
        let msg = model_msg.clone();
        tokio::task::spawn_blocking(move || store.add_message(&sid, &msg))
            .await
            .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
            .map_err(|e| ServerFnError::new(e))?;
    }

    Ok(model_msg)
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Retrieve a session's model name and all messages for export.
///
/// Returns `(model_name, messages)` so the client can render the session as
/// Markdown, HTML, or JSON without additional round-trips.
#[server(ExportSession)]
pub async fn export_session(
    session_id: String,
) -> Result<(String, Vec<ChatMessage>), ServerFnError> {
    let Extension(store) =
        leptos_axum::extract::<Extension<Arc<SessionStore>>>().await?;

    let store2 = store.clone();
    let sid = session_id.clone();
    let session = tokio::task::spawn_blocking(move || store2.get_session(&sid))
        .await
        .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
        .map_err(|e| ServerFnError::new(e))?
        .ok_or_else(|| ServerFnError::new(format!("session not found: {session_id}")))?;

    let messages = tokio::task::spawn_blocking(move || store.get_messages(&session.id))
        .await
        .map_err(|e| ServerFnError::new(format!("join error: {e}")))?
        .map_err(|e| ServerFnError::new(e))?;

    Ok((session.model_name, messages))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert an `InferenceInput` into a `MessageContent` for persistence.
#[cfg(feature = "ssr")]
fn input_to_content(input: &InferenceInput) -> MessageContent {
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
            // For user-side persistence, show the first image with a caption
            // indicating the batch size. The full batch results get stored as
            // MessageContent::Batch in the model response.
            if let Some(first) = images.first() {
                MessageContent::Image {
                    data_base64: first.data_base64.clone(),
                    mime_type: first.mime_type.clone(),
                    caption: Some(
                        prompt
                            .clone()
                            .unwrap_or_else(|| format!("Batch: {} images", images.len())),
                    ),
                }
            } else {
                MessageContent::Text(
                    prompt.clone().unwrap_or_else(|| "Empty batch".to_string()),
                )
            }
        }
    }
}
