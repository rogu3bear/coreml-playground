//! Integration tests for ModelRegistry — validates model discovery,
//! listing, and mock fallback behavior.

#![cfg(feature = "ssr")]

use coreml_playground::server::model_registry::ModelRegistry;
use coreml_playground::types::ModelType;

/// Helper: create a ModelRegistry pointing at a fresh, empty temp directory.
/// Since the directory contains no `.mlmodel` or `.mlpackage` files the
/// registry will fall back to its built-in mock models.
///
/// Construction is performed inside `spawn_blocking` because
/// `ModelRegistry::new` calls `blocking_write()` on a tokio `RwLock`, which
/// panics if invoked directly on an async runtime thread.
async fn registry_with_empty_dir() -> (ModelRegistry, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let path = dir.path().to_str().expect("non-UTF-8 temp path").to_owned();
    let registry = tokio::task::spawn_blocking(move || ModelRegistry::new(&path))
        .await
        .expect("spawn_blocking should not panic");
    (registry, dir)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_empty_directory_returns_mock_models() {
    let (registry, _dir) = registry_with_empty_dir().await;

    let models = registry.list_models().await;

    // The registry defines 3 mock models + 1 demo-echo model.
    assert_eq!(
        models.len(),
        4,
        "empty directory should yield 3 mock models + 1 demo model, got {}",
        models.len()
    );

    let names: Vec<&str> = models.iter().map(|m| m.name.as_str()).collect();
    assert!(
        names.contains(&"MobileNetV2"),
        "mock models should include MobileNetV2, got {:?}",
        names
    );
    assert!(
        names.contains(&"TextSentiment"),
        "mock models should include TextSentiment, got {:?}",
        names
    );
    assert!(
        names.contains(&"WhisperTiny"),
        "mock models should include WhisperTiny, got {:?}",
        names
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_list_models_returns_valid_info() {
    let (registry, _dir) = registry_with_empty_dir().await;

    let models = registry.list_models().await;

    for model in &models {
        assert!(
            !model.id.is_empty(),
            "model id should be non-empty for '{}'",
            model.name
        );
        assert!(
            !model.name.is_empty(),
            "model name should be non-empty for id '{}'",
            model.id
        );

        // ModelType should be one of the known variants (not Unknown for mocks).
        let valid_types = [
            ModelType::Vision,
            ModelType::Text,
            ModelType::Audio,
            ModelType::Multimodal,
        ];
        assert!(
            valid_types.contains(&model.model_type),
            "mock model '{}' should have a concrete ModelType, got {:?}",
            model.name,
            model.model_type
        );

        assert!(
            !model.input_schema.is_empty(),
            "model '{}' should have non-empty input_schema",
            model.name
        );
        assert!(
            !model.output_schema.is_empty(),
            "model '{}' should have non-empty output_schema",
            model.name
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_model_types_are_correct() {
    let (registry, _dir) = registry_with_empty_dir().await;

    let models = registry.list_models().await;

    let find = |name: &str| {
        models
            .iter()
            .find(|m| m.name == name)
            .unwrap_or_else(|| panic!("mock model '{}' should be present", name))
    };

    assert_eq!(
        find("MobileNetV2").model_type,
        ModelType::Vision,
        "MobileNetV2 should be Vision"
    );
    assert_eq!(
        find("TextSentiment").model_type,
        ModelType::Text,
        "TextSentiment should be Text"
    );
    assert_eq!(
        find("WhisperTiny").model_type,
        ModelType::Audio,
        "WhisperTiny should be Audio"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_load_model_returns_info() {
    let (registry, _dir) = registry_with_empty_dir().await;

    let result = registry.load_model("mock-mobilenetv2").await;
    assert!(
        result.is_ok(),
        "load_model should succeed for a mock model, got {:?}",
        result.err()
    );

    let info = result.unwrap();
    assert_eq!(info.id, "mock-mobilenetv2", "loaded model id should match");
    assert_eq!(info.name, "MobileNetV2", "loaded model name should match");
    assert_eq!(
        info.model_type,
        ModelType::Vision,
        "loaded model type should be Vision"
    );
}
