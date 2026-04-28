//! Round-trip tests: feed each mock model's actual JSON output through the
//! interpreter and verify the correct detection branch fires.
//!
//! No feature gate needed — `server::interpreter` compiles under both SSR
//! and WASM targets.

use coreml_playground::server::interpreter::interpret_output;
use coreml_playground::types::{ModelType, PortInfo};
use serde_json::json;

fn empty_schema() -> Vec<PortInfo> {
    vec![]
}

// ---------------------------------------------------------------------------
// mobilenetv2 mock output -> classification branch
// ---------------------------------------------------------------------------

#[test]
fn mobilenetv2_hits_classification() {
    let output = json!({
        "classLabel": "golden retriever",
        "classLabelProbs": {
            "golden retriever": 0.87,
            "Labrador retriever": 0.08,
            "cocker spaniel": 0.03,
        }
    });

    let result = interpret_output(&output, &ModelType::Vision, &empty_schema());

    assert!(
        result.summary.contains("golden retriever"),
        "summary should mention the top class, got: {}",
        result.summary
    );
    assert!(
        result.summary.contains("87%"),
        "summary should mention 87% confidence, got: {}",
        result.summary
    );

    let detail = result.detail.as_deref().unwrap_or("");
    assert!(
        detail.contains("Labrador"),
        "detail should list other classes, got: {detail}"
    );
}

// ---------------------------------------------------------------------------
// textsentiment mock output -> sentiment branch
// ---------------------------------------------------------------------------

#[test]
fn textsentiment_positive_hits_sentiment() {
    let output = json!({
        "label": "Positive",
        "score": 0.92,
    });

    let result = interpret_output(&output, &ModelType::Text, &empty_schema());

    assert!(
        result.summary.contains("Positive"),
        "summary should contain 'Positive', got: {}",
        result.summary
    );
    assert!(
        result.summary.contains("0.92"),
        "summary should contain the score, got: {}",
        result.summary
    );
    assert!(result.detail.is_none(), "sentiment should have no detail");
}

#[test]
fn textsentiment_negative_hits_sentiment() {
    let output = json!({
        "label": "Negative",
        "score": 0.78,
    });

    let result = interpret_output(&output, &ModelType::Text, &empty_schema());

    assert!(
        result.summary.contains("Negative"),
        "summary should contain 'Negative', got: {}",
        result.summary
    );
}

// ---------------------------------------------------------------------------
// whispertiny mock output -> transcription branch
// ---------------------------------------------------------------------------

#[test]
fn whispertiny_hits_transcription() {
    let output = json!({
        "text": "This is a mock transcription of the provided audio clip."
    });

    let result = interpret_output(&output, &ModelType::Audio, &empty_schema());

    assert!(
        result.summary.contains("mock transcription"),
        "summary should contain the transcript text, got: {}",
        result.summary
    );
}

// ---------------------------------------------------------------------------
// demo-echo mock output -> detect_echo branch
// ---------------------------------------------------------------------------

#[test]
fn demo_echo_hits_detect_echo() {
    let output = json!({
        "echo": "Hello world",
        "note": "This is a demo model. Load a real CoreML model to get started."
    });

    let result = interpret_output(&output, &ModelType::Unknown, &empty_schema());

    assert_eq!(
        result.summary, "Hello world",
        "summary should be the echoed text"
    );
    assert_eq!(
        result.detail.as_deref(),
        Some("This is a demo model. Load a real CoreML model to get started."),
        "detail should be the note"
    );
}

#[test]
fn detect_echo_without_note() {
    let output = json!({ "echo": "just the echo" });

    let result = interpret_output(&output, &ModelType::Unknown, &empty_schema());

    assert_eq!(result.summary, "just the echo");
    assert!(result.detail.is_none(), "no note means no detail");
}

// ---------------------------------------------------------------------------
// Unknown model fallback -> actually hits fallback()
// ---------------------------------------------------------------------------

#[test]
fn unknown_model_hits_fallback() {
    // The unknown-model mock returns mixed types (array + object) that no
    // detector matches, exercising the fallback path.
    let output = json!({
        "output_0": [0.5, 0.3, 0.2],
        "debug": { "model": "unknown", "mock": true }
    });

    let result = interpret_output(&output, &ModelType::Unknown, &empty_schema());

    assert!(
        result.summary.contains("2 output fields"),
        "mixed-type output should hit fallback, got: {}",
        result.summary
    );
    assert!(
        result.detail.is_some(),
        "fallback should include pretty-printed JSON detail"
    );
}
