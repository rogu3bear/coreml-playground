//! Tests for the pure functions in types.rs.
//!
//! These work on both SSR and WASM targets — no `#[cfg]` gates needed.

use coreml_playground::types::*;

// ---------------------------------------------------------------------------
// ModelType tests
// ---------------------------------------------------------------------------

#[test]
fn test_model_type_labels() {
    assert_eq!(ModelType::Text.label(), "Text", "Text label");
    assert_eq!(ModelType::Vision.label(), "Vision", "Vision label");
    assert_eq!(
        ModelType::Multimodal.label(),
        "Multimodal",
        "Multimodal label"
    );
    assert_eq!(ModelType::Audio.label(), "Audio", "Audio label");
    assert_eq!(ModelType::Unknown.label(), "Unknown", "Unknown label");
}

#[test]
fn test_model_type_accepts_image() {
    assert!(
        ModelType::Vision.accepts_image(),
        "Vision should accept images"
    );
    assert!(
        ModelType::Multimodal.accepts_image(),
        "Multimodal should accept images"
    );
    assert!(
        !ModelType::Text.accepts_image(),
        "Text should not accept images"
    );
    assert!(
        !ModelType::Audio.accepts_image(),
        "Audio should not accept images"
    );
    assert!(
        !ModelType::Unknown.accepts_image(),
        "Unknown should not accept images"
    );
}

#[test]
fn test_model_type_accepts_text() {
    assert!(ModelType::Text.accepts_text(), "Text should accept text");
    assert!(
        ModelType::Multimodal.accepts_text(),
        "Multimodal should accept text"
    );
    assert!(
        !ModelType::Vision.accepts_text(),
        "Vision should not accept text"
    );
    assert!(
        !ModelType::Audio.accepts_text(),
        "Audio should not accept text"
    );
    assert!(
        !ModelType::Unknown.accepts_text(),
        "Unknown should not accept text"
    );
}

#[test]
fn test_model_type_is_chat_compatible() {
    assert!(
        ModelType::Text.is_chat_compatible(),
        "Text should be chat compatible"
    );
    assert!(
        ModelType::Vision.is_chat_compatible(),
        "Vision should be chat compatible"
    );
    assert!(
        ModelType::Multimodal.is_chat_compatible(),
        "Multimodal should be chat compatible"
    );
    assert!(
        !ModelType::Audio.is_chat_compatible(),
        "Audio should not be chat compatible"
    );
    assert!(
        !ModelType::Unknown.is_chat_compatible(),
        "Unknown should not be chat compatible"
    );
}

// ---------------------------------------------------------------------------
// MessageContent tests
// ---------------------------------------------------------------------------

#[test]
fn test_message_content_preview() {
    // Text variant
    let text = MessageContent::Text("Hello, world!".into());
    assert_eq!(
        text.preview(80),
        "Hello, world!",
        "Text preview should return the text"
    );

    // Image with caption
    let img_with_caption = MessageContent::Image {
        data_base64: "abc".into(),
        mime_type: "image/png".into(),
        caption: Some("a cute cat".into()),
    };
    assert_eq!(
        img_with_caption.preview(80),
        "a cute cat",
        "Image preview should return caption when present"
    );

    // Image without caption
    let img_no_caption = MessageContent::Image {
        data_base64: "abc".into(),
        mime_type: "image/png".into(),
        caption: None,
    };
    assert_eq!(
        img_no_caption.preview(80),
        "[image]",
        "Image preview should return '[image]' when no caption"
    );

    // ModelOutput
    let output = MessageContent::ModelOutput(serde_json::json!({"label": "cat"}));
    assert_eq!(
        output.preview(80),
        "[model output]",
        "ModelOutput preview should return '[model output]'"
    );

    // Streaming
    let streaming = MessageContent::Streaming {
        partial: "generating response".into(),
        done: false,
    };
    assert_eq!(
        streaming.preview(80),
        "generating response",
        "Streaming preview should return the partial text"
    );
}

#[test]
fn test_truncate_behavior() {
    let long_text = MessageContent::Text("a".repeat(200));

    let preview = long_text.preview(10);
    assert!(
        preview.ends_with("..."),
        "truncated preview should end with '...', got '{}'",
        preview
    );
    // The prefix before "..." should be at most `max_len` chars.
    assert!(
        preview.len() <= 10 + 3,
        "truncated preview should be at most max_len + 3 chars, got len {}",
        preview.len()
    );

    // Short text should not be truncated.
    let short_text = MessageContent::Text("hi".into());
    let short_preview = short_text.preview(10);
    assert_eq!(short_preview, "hi", "short text should not be truncated");
    assert!(
        !short_preview.ends_with("..."),
        "short text preview should not end with '...'"
    );
}

#[test]
fn test_message_content_as_text() {
    // Text variant should return Some.
    let text = MessageContent::Text("hello".into());
    assert_eq!(
        text.as_text(),
        Some("hello"),
        "Text.as_text() should return Some"
    );

    // Streaming variant should return Some.
    let streaming = MessageContent::Streaming {
        partial: "partial output".into(),
        done: false,
    };
    assert_eq!(
        streaming.as_text(),
        Some("partial output"),
        "Streaming.as_text() should return Some"
    );

    // Image variant should return None.
    let image = MessageContent::Image {
        data_base64: "abc".into(),
        mime_type: "image/png".into(),
        caption: Some("caption".into()),
    };
    assert!(
        image.as_text().is_none(),
        "Image.as_text() should return None"
    );

    // ModelOutput variant should return None.
    let output = MessageContent::ModelOutput(serde_json::json!({}));
    assert!(
        output.as_text().is_none(),
        "ModelOutput.as_text() should return None"
    );
}

// ---------------------------------------------------------------------------
// Latency helper tests
// ---------------------------------------------------------------------------

#[test]
fn test_describe_latency() {
    assert_eq!(describe_latency(0), "instant");
    assert_eq!(describe_latency(49), "instant");
    assert_eq!(describe_latency(50), "faster than a blink");
    assert_eq!(describe_latency(199), "faster than a blink");
    assert_eq!(describe_latency(200), "quick");
    assert_eq!(describe_latency(499), "quick");
    assert_eq!(describe_latency(500), "a moment");
    assert_eq!(describe_latency(999), "a moment");
    assert_eq!(describe_latency(1000), "working on it");
    assert_eq!(describe_latency(2999), "working on it");
    assert_eq!(describe_latency(3000), "heavy lifting");
    assert_eq!(describe_latency(10000), "heavy lifting");
}

#[test]
fn test_latency_display() {
    let display = latency_display(150);
    assert!(
        display.contains("150ms"),
        "latency_display should contain the ms value, got '{}'",
        display
    );
    assert!(
        display.contains("faster than a blink"),
        "latency_display should contain the description, got '{}'",
        display
    );
}
