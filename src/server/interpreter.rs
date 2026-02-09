use crate::types::{ModelType, PortInfo};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FormattedResponse {
    /// One-line human-readable summary
    pub summary: String,
    /// Structured breakdown (may be multi-line)
    pub detail: Option<String>,
    /// Original raw JSON output
    pub raw: serde_json::Value,
}

/// Transform raw CoreML model output into a human-readable `FormattedResponse`.
///
/// Detection is attempted in priority order: classification, sentiment,
/// object detection, transcription, text, embedding, regression, and finally
/// a generic fallback.
pub fn interpret_output(
    output: &serde_json::Value,
    _model_type: &ModelType,
    _output_schema: &[PortInfo],
) -> FormattedResponse {
    if let Some(r) = detect_classification(output) {
        return r;
    }
    if let Some(r) = detect_sentiment(output) {
        return r;
    }
    if let Some(r) = detect_object_detection(output) {
        return r;
    }
    if let Some(r) = detect_transcription(output) {
        return r;
    }
    if let Some(r) = detect_text(output) {
        return r;
    }
    if let Some(r) = detect_embedding(output) {
        return r;
    }
    if let Some(r) = detect_regression(output) {
        return r;
    }

    fallback(output)
}

// ---------------------------------------------------------------------------
// Detection helpers
// ---------------------------------------------------------------------------

/// Classification: output has "classLabel" (string) + "classLabelProbs" (dict),
/// OR the output itself is a dict mapping string keys to float values.
fn detect_classification(output: &serde_json::Value) -> Option<FormattedResponse> {
    // Variant 1: explicit classLabel + classLabelProbs
    if let (Some(label), Some(probs)) = (
        output.get("classLabel").and_then(|v| v.as_str()),
        output
            .get("classLabelProbs")
            .and_then(|v| v.as_object()),
    ) {
        let confidence = probs
            .get(label)
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            * 100.0;

        let mut sorted: Vec<(&str, f64)> = probs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_f64().unwrap_or(0.0) * 100.0))
            .collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let summary = format!("I see a **{}** ({:.0}% confidence)", label, confidence);

        let others: Vec<String> = sorted
            .iter()
            .filter(|(k, _)| *k != label)
            .take(4)
            .map(|(k, c)| format!("{} ({:.0}%)", k, c))
            .collect();

        let detail = if others.is_empty() {
            None
        } else {
            Some(format!("Other possibilities: {}", others.join(", ")))
        };

        return Some(FormattedResponse {
            summary,
            detail,
            raw: output.clone(),
        });
    }

    // Variant 2: bare dict of string -> float (all values are numbers)
    if let Some(obj) = output.as_object() {
        if obj.len() >= 2 && obj.values().all(|v| v.is_f64() || v.is_i64() || v.is_u64()) {
            let mut sorted: Vec<(&str, f64)> = obj
                .iter()
                .map(|(k, v)| (k.as_str(), as_f64(v) * 100.0))
                .collect();
            sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            if let Some(&(top_label, top_conf)) = sorted.first() {
                let summary =
                    format!("I see a **{}** ({:.0}% confidence)", top_label, top_conf);

                let others: Vec<String> = sorted
                    .iter()
                    .skip(1)
                    .take(4)
                    .map(|(k, c)| format!("{} ({:.0}%)", k, c))
                    .collect();

                let detail = if others.is_empty() {
                    None
                } else {
                    Some(format!("Other possibilities: {}", others.join(", ")))
                };

                return Some(FormattedResponse {
                    summary,
                    detail,
                    raw: output.clone(),
                });
            }
        }
    }

    None
}

/// Sentiment: output has "label" (string) + "score" (number).
fn detect_sentiment(output: &serde_json::Value) -> Option<FormattedResponse> {
    let label = output.get("label").and_then(|v| v.as_str())?;
    let score = output.get("score").and_then(|v| v.as_f64())?;

    Some(FormattedResponse {
        summary: format!("This text feels **{}** (score: {:.2})", label, score),
        detail: None,
        raw: output.clone(),
    })
}

/// Object detection: output has "coordinates" array OR "boxes"/"rects" key.
fn detect_object_detection(output: &serde_json::Value) -> Option<FormattedResponse> {
    let detections = output
        .get("coordinates")
        .or_else(|| output.get("boxes"))
        .or_else(|| output.get("rects"))
        .and_then(|v| v.as_array())?;

    let n = detections.len();

    // Try to build per-object summaries from sibling "labels"/"confidences" arrays
    let labels = output
        .get("labels")
        .or_else(|| output.get("classes"))
        .and_then(|v| v.as_array());
    let confidences = output
        .get("confidences")
        .or_else(|| output.get("scores"))
        .and_then(|v| v.as_array());

    let mut obj_descs: Vec<String> = Vec::new();
    let mut detail_lines: Vec<String> = Vec::new();

    for i in 0..n {
        let label = labels
            .and_then(|arr| arr.get(i))
            .and_then(|v| v.as_str())
            .unwrap_or("object");
        let conf = confidences
            .and_then(|arr| arr.get(i))
            .and_then(|v| v.as_f64())
            .map(|c| c * 100.0);

        let desc = match conf {
            Some(c) => format!("{} ({:.0}%)", label, c),
            None => label.to_string(),
        };
        obj_descs.push(desc);

        // Bounding box detail
        if let Some(coords) = detections.get(i) {
            detail_lines.push(format!("  {}: bbox {}", label, coords));
        }
    }

    let summary = format!("I found {} objects: {}", n, obj_descs.join(", "));
    let detail = if detail_lines.is_empty() {
        None
    } else {
        Some(detail_lines.join("\n"))
    };

    Some(FormattedResponse {
        summary,
        detail,
        raw: output.clone(),
    })
}

/// Transcription: output has "text" or "transcription" key (string).
fn detect_transcription(output: &serde_json::Value) -> Option<FormattedResponse> {
    let text = output
        .get("transcription")
        .or_else(|| output.get("text"))
        .and_then(|v| v.as_str())?;

    Some(FormattedResponse {
        summary: format!("\"{}\"", text),
        detail: None,
        raw: output.clone(),
    })
}

/// Text: output is itself a string, or a single-key object whose value is a string.
fn detect_text(output: &serde_json::Value) -> Option<FormattedResponse> {
    if let Some(s) = output.as_str() {
        return Some(FormattedResponse {
            summary: s.to_string(),
            detail: None,
            raw: output.clone(),
        });
    }

    if let Some(obj) = output.as_object() {
        if obj.len() == 1 {
            if let Some(s) = obj.values().next().and_then(|v| v.as_str()) {
                return Some(FormattedResponse {
                    summary: s.to_string(),
                    detail: None,
                    raw: output.clone(),
                });
            }
        }
    }

    None
}

/// Embedding / feature vector: output is a single array of numbers with >100 elements.
fn detect_embedding(output: &serde_json::Value) -> Option<FormattedResponse> {
    let arr = as_number_array(output)?;
    if arr.len() <= 100 {
        return None;
    }

    let dim = arr.len();

    // Find top 5 activations by absolute value
    let mut indexed: Vec<(usize, f64)> = arr.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| {
        b.1.abs()
            .partial_cmp(&a.1.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top5: Vec<String> = indexed
        .iter()
        .take(5)
        .map(|(i, v)| format!("[{}]={:.4}", i, v))
        .collect();

    Some(FormattedResponse {
        summary: format!("Generated a {}-dimensional feature vector", dim),
        detail: Some(format!("Top 5 activations: {}", top5.join(", "))),
        raw: output.clone(),
    })
}

/// Regression: output is a single number.
fn detect_regression(output: &serde_json::Value) -> Option<FormattedResponse> {
    let val = output.as_f64().or_else(|| output.as_i64().map(|i| i as f64))?;

    Some(FormattedResponse {
        summary: format!("Predicted value: {:.4}", val),
        detail: None,
        raw: output.clone(),
    })
}

/// Fallback when no pattern matched.
fn fallback(output: &serde_json::Value) -> FormattedResponse {
    let n = match output.as_object() {
        Some(obj) => obj.len(),
        None => 1,
    };

    let pretty = serde_json::to_string_pretty(output).unwrap_or_else(|_| output.to_string());
    let detail = if pretty.len() > 500 {
        format!("{}...", &pretty[..500])
    } else {
        pretty
    };

    FormattedResponse {
        summary: format!("Model returned {} output fields", n),
        detail: Some(detail),
        raw: output.clone(),
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn as_f64(v: &serde_json::Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_u64().map(|u| u as f64))
        .unwrap_or(0.0)
}

/// If `output` is a flat array of numbers, return the values.
/// Also handles a single-key object wrapping such an array.
fn as_number_array(output: &serde_json::Value) -> Option<Vec<f64>> {
    if let Some(arr) = output.as_array() {
        let nums: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
        if nums.len() == arr.len() && !nums.is_empty() {
            return Some(nums);
        }
    }

    // Single-key wrapper
    if let Some(obj) = output.as_object() {
        if obj.len() == 1 {
            if let Some(arr) = obj.values().next().and_then(|v| v.as_array()) {
                let nums: Vec<f64> = arr.iter().filter_map(|v| v.as_f64()).collect();
                if nums.len() == arr.len() && !nums.is_empty() {
                    return Some(nums);
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dummy_schema() -> Vec<PortInfo> {
        vec![]
    }

    #[test]
    fn test_classification_with_class_label() {
        let output = json!({
            "classLabel": "golden retriever",
            "classLabelProbs": {
                "golden retriever": 0.92,
                "labrador": 0.05,
                "poodle": 0.02,
                "beagle": 0.01
            }
        });

        let result = interpret_output(&output, &ModelType::Vision, &dummy_schema());
        assert!(result.summary.contains("golden retriever"));
        assert!(result.summary.contains("92%"));
        let detail = result.detail.unwrap();
        assert!(detail.contains("labrador"));
        assert!(detail.contains("poodle"));
    }

    #[test]
    fn test_classification_bare_dict() {
        let output = json!({
            "cat": 0.85,
            "dog": 0.10,
            "bird": 0.05
        });

        let result = interpret_output(&output, &ModelType::Vision, &dummy_schema());
        assert!(result.summary.contains("cat"));
        assert!(result.summary.contains("85%"));
        assert!(result.detail.is_some());
    }

    #[test]
    fn test_sentiment() {
        let output = json!({
            "label": "positive",
            "score": 0.97
        });

        let result = interpret_output(&output, &ModelType::Text, &dummy_schema());
        assert!(result.summary.contains("positive"));
        assert!(result.summary.contains("0.97"));
        assert!(result.detail.is_none());
    }

    #[test]
    fn test_embedding_detection() {
        // 128-dimensional vector
        let values: Vec<f64> = (0..128).map(|i| (i as f64) * 0.01).collect();
        let output = json!(values);

        let result = interpret_output(&output, &ModelType::Vision, &dummy_schema());
        assert!(result.summary.contains("128-dimensional"));
        assert!(result.detail.is_some());
        let detail = result.detail.unwrap();
        assert!(detail.contains("Top 5 activations"));
    }

    #[test]
    fn test_small_array_not_embedding() {
        // 10 elements should NOT be detected as embedding
        let values: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let output = json!(values);

        let result = interpret_output(&output, &ModelType::Vision, &dummy_schema());
        // Should fall through to fallback, not embedding
        assert!(!result.summary.contains("dimensional"));
    }

    #[test]
    fn test_text_passthrough_string() {
        let output = json!("Hello, this is the model output.");

        let result = interpret_output(&output, &ModelType::Text, &dummy_schema());
        assert_eq!(result.summary, "Hello, this is the model output.");
        assert!(result.detail.is_none());
    }

    #[test]
    fn test_text_passthrough_single_key() {
        let output = json!({ "generated_text": "The weather is sunny." });

        let result = interpret_output(&output, &ModelType::Text, &dummy_schema());
        assert_eq!(result.summary, "The weather is sunny.");
        assert!(result.detail.is_none());
    }

    #[test]
    fn test_transcription() {
        let output = json!({ "transcription": "Hello world, testing one two three." });

        let result = interpret_output(&output, &ModelType::Audio, &dummy_schema());
        assert!(result.summary.starts_with('"'));
        assert!(result.summary.contains("Hello world"));
    }

    #[test]
    fn test_transcription_text_key() {
        let output = json!({ "text": "Speech recognition result." });

        let result = interpret_output(&output, &ModelType::Audio, &dummy_schema());
        assert!(result.summary.contains("Speech recognition result"));
    }

    #[test]
    fn test_regression() {
        let output = json!(3.14159);

        let result = interpret_output(&output, &ModelType::Unknown, &dummy_schema());
        assert!(result.summary.contains("3.1416"));
        assert!(result.detail.is_none());
    }

    #[test]
    fn test_object_detection() {
        let output = json!({
            "coordinates": [[10, 20, 100, 200], [50, 60, 150, 250]],
            "labels": ["person", "car"],
            "confidences": [0.95, 0.87]
        });

        let result = interpret_output(&output, &ModelType::Vision, &dummy_schema());
        assert!(result.summary.contains("2 objects"));
        assert!(result.summary.contains("person"));
        assert!(result.summary.contains("car"));
        assert!(result.detail.is_some());
    }

    #[test]
    fn test_fallback_unknown_structure() {
        let output = json!({
            "foo": [1, 2, 3],
            "bar": { "nested": true },
            "baz": "hello"
        });

        let result = interpret_output(&output, &ModelType::Unknown, &dummy_schema());
        assert!(result.summary.contains("3 output fields"));
        assert!(result.detail.is_some());
    }
}
