//! Tests for the inference module — mock predictions and demo echo.
//!
//! Unit tests for `input_to_content` live inside `src/server/inference.rs`
//! (the function is `pub(crate)`, invisible to integration tests).

#![cfg(feature = "ssr")]

use std::sync::Arc;

use coreml_playground::server::inference::run_inference_for_api;
use coreml_playground::server::model_registry::ModelRegistry;
use coreml_playground::types::*;

/// Create a ModelRegistry backed by a non-existent directory (so only mock
/// models are available) and load the given model.
///
/// `ModelRegistry::new` uses `blocking_write()` on a tokio RwLock, which
/// panics if called from within an async context. We use `spawn_blocking`
/// to sidestep this.
async fn registry_with_model(model_id: &str) -> Arc<ModelRegistry> {
    let registry = Arc::new(
        tokio::task::spawn_blocking(|| {
            ModelRegistry::new("/tmp/__coreml_test_nonexistent_models__")
        })
        .await
        .expect("spawn_blocking failed"),
    );
    registry
        .load_model(model_id)
        .await
        .unwrap_or_else(|e| panic!("failed to load model {model_id}: {e}"));
    registry
}

// ---------------------------------------------------------------------------
// Demo echo model
// ---------------------------------------------------------------------------

#[tokio::test]
async fn demo_echo_text_input() {
    let registry = registry_with_model("demo-echo").await;
    let input = InferenceInput::Text("Hello world".into());

    let output = run_inference_for_api(&registry, &input)
        .await
        .expect("demo-echo should succeed");

    assert_eq!(
        output.get("echo").and_then(|v| v.as_str()),
        Some("Hello world"),
        "echo field should mirror input text"
    );
    assert!(
        output.get("note").is_some(),
        "demo-echo should include a note field"
    );
}

#[tokio::test]
async fn demo_echo_image_input() {
    let registry = registry_with_model("demo-echo").await;
    let input = InferenceInput::Image {
        data_base64: "abc123".into(),
        mime_type: "image/jpeg".into(),
        prompt: None,
    };

    let output = run_inference_for_api(&registry, &input)
        .await
        .expect("demo-echo should succeed for image input");

    let echo = output
        .get("echo")
        .and_then(|v| v.as_str())
        .expect("echo field should exist");
    assert!(
        echo.contains("image/jpeg"),
        "echo should mention the mime type, got: {echo}"
    );
}

// ---------------------------------------------------------------------------
// Mock models
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_mobilenetv2_output_shape() {
    let registry = registry_with_model("mock-mobilenetv2").await;
    let input = InferenceInput::Image {
        data_base64: "dGVzdA==".into(),
        mime_type: "image/png".into(),
        prompt: None,
    };

    let output = run_inference_for_api(&registry, &input)
        .await
        .expect("mock-mobilenetv2 should succeed");

    assert!(
        output.get("classLabel").and_then(|v| v.as_str()).is_some(),
        "mobilenetv2 output should have classLabel"
    );
    assert!(
        output
            .get("classLabelProbs")
            .and_then(|v| v.as_object())
            .is_some(),
        "mobilenetv2 output should have classLabelProbs"
    );
}

#[tokio::test]
async fn mock_textsentiment_positive() {
    let registry = registry_with_model("mock-textsentiment").await;
    let input = InferenceInput::Text("I love this product, it's great!".into());

    let output = run_inference_for_api(&registry, &input)
        .await
        .expect("mock-textsentiment should succeed");

    assert_eq!(
        output.get("label").and_then(|v| v.as_str()),
        Some("Positive"),
    );
    assert!(output.get("score").and_then(|v| v.as_f64()).is_some());
}

#[tokio::test]
async fn mock_textsentiment_negative() {
    let registry = registry_with_model("mock-textsentiment").await;
    let input = InferenceInput::Text("This is terrible and broken".into());

    let output = run_inference_for_api(&registry, &input)
        .await
        .expect("mock-textsentiment should succeed");

    assert_eq!(
        output.get("label").and_then(|v| v.as_str()),
        Some("Negative"),
    );
}

#[tokio::test]
async fn mock_whispertiny_output_shape() {
    let registry = registry_with_model("mock-whispertiny").await;
    let input = InferenceInput::Text("audio data".into());

    let output = run_inference_for_api(&registry, &input)
        .await
        .expect("mock-whispertiny should succeed");

    assert!(
        output.get("text").and_then(|v| v.as_str()).is_some(),
        "whispertiny output should have a text field"
    );
}

#[tokio::test]
async fn mock_textsentiment_image_with_prompt() {
    let registry = registry_with_model("mock-textsentiment").await;
    let input = InferenceInput::Image {
        data_base64: "dGVzdA==".into(),
        mime_type: "image/png".into(),
        prompt: Some("I love this picture".into()),
    };

    let output = run_inference_for_api(&registry, &input)
        .await
        .expect("mock-textsentiment should succeed for image with prompt");

    assert_eq!(
        output.get("label").and_then(|v| v.as_str()),
        Some("Positive"),
        "prompt containing 'love' should produce Positive sentiment"
    );
}

#[tokio::test]
async fn mock_textsentiment_image_no_prompt() {
    let registry = registry_with_model("mock-textsentiment").await;
    let input = InferenceInput::Image {
        data_base64: "dGVzdA==".into(),
        mime_type: "image/png".into(),
        prompt: None,
    };

    let output = run_inference_for_api(&registry, &input)
        .await
        .expect("mock-textsentiment should succeed for image without prompt");

    assert_eq!(
        output.get("label").and_then(|v| v.as_str()),
        Some("Negative"),
        "image with no prompt text should default to Negative sentiment"
    );
}

#[tokio::test]
async fn mock_unknown_model_not_in_registry() {
    // An unknown model ID should fail at the registry level — it's not
    // registered, so load_model returns an error.
    let registry = Arc::new(
        tokio::task::spawn_blocking(|| {
            ModelRegistry::new("/tmp/__coreml_test_nonexistent_models__")
        })
        .await
        .expect("spawn_blocking failed"),
    );
    let result = registry.load_model("totally-unknown-model").await;

    assert!(
        result.is_err(),
        "loading an unregistered model should fail, got: {:?}",
        result
    );
}
