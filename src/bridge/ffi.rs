// CoreML FFI bindings
//
// Safe Rust wrappers around the C-compatible functions exported by
// swift/CoreMLBridge.swift. All types and functions here are gated behind
// `#[cfg(feature = "ssr")]` at the module level (see bridge/mod.rs).

use std::ffi::{c_char, c_void, CStr, CString};

use crate::types::{ModelInfo, ModelType, PortInfo};

// ---------------------------------------------------------------------------
// Raw FFI declarations
// ---------------------------------------------------------------------------

extern "C" {
    fn coreml_load_model(path: *const c_char) -> *mut c_void;
    fn coreml_unload_model(handle: *mut c_void);
    fn coreml_get_metadata(handle: *mut c_void) -> *const c_char;
    fn coreml_predict_text(handle: *mut c_void, input_json: *const c_char) -> *const c_char;
    fn coreml_predict_image(
        handle: *mut c_void,
        image_data: *const u8,
        image_len: usize,
        prompt: *const c_char,
    ) -> *const c_char;
    fn coreml_free_string(ptr: *const c_char);
}

// ---------------------------------------------------------------------------
// ModelHandle — opaque, owned pointer to a loaded CoreML model
// ---------------------------------------------------------------------------

/// Opaque handle to a loaded CoreML model on the Swift side.
///
/// The inner pointer is produced by `coreml_load_model` and must be released
/// exactly once via `coreml_unload_model`. CoreML models are thread-safe after
/// compilation, so we mark this as `Send + Sync`.
pub struct ModelHandle(*mut c_void);

// SAFETY: CoreML models are thread-safe for prediction after loading.
// The Swift side does not mutate shared state during inference calls.
unsafe impl Send for ModelHandle {}
unsafe impl Sync for ModelHandle {}

impl Drop for ModelHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { coreml_unload_model(self.0) }
        }
    }
}

impl ModelHandle {
    /// Returns the raw pointer for passing into FFI calls.
    fn as_ptr(&self) -> *mut c_void {
        self.0
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Read a C string returned by Swift, copy it into a Rust `String`, and free
/// the Swift-allocated memory.
///
/// Returns `Err` if the pointer is null or the bytes are not valid UTF-8.
unsafe fn read_and_free_cstring(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("Swift returned a null pointer".into());
    }
    let cstr = unsafe { CStr::from_ptr(ptr) };
    let result = cstr
        .to_str()
        .map(|s| s.to_owned())
        .map_err(|e| format!("Invalid UTF-8 from Swift: {e}"));
    unsafe { coreml_free_string(ptr) };
    result
}

/// Check whether a JSON string from Swift is an error envelope (`{"error": "..."}`).
/// If so, extract the message and return it as `Err`.
fn check_error_json(json: &str) -> Result<(), String> {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json) {
        if let Some(msg) = val.get("error").and_then(|v| v.as_str()) {
            return Err(msg.to_owned());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Public safe API
// ---------------------------------------------------------------------------

/// Load a CoreML model from a filesystem path.
///
/// Supports `.mlmodel`, `.mlmodelc`, and `.mlpackage` files. For `.mlpackage`
/// the Swift layer compiles the model to a temporary directory on first load.
///
/// # Errors
///
/// Returns an error if the path is not valid UTF-8, the file does not exist,
/// or the model fails to compile/load.
pub fn load_model(path: &str) -> Result<ModelHandle, String> {
    let c_path = CString::new(path).map_err(|e| format!("Invalid path string: {e}"))?;
    let ptr = unsafe { coreml_load_model(c_path.as_ptr()) };
    if ptr.is_null() {
        return Err(format!("Failed to load model at: {path}"));
    }
    Ok(ModelHandle(ptr))
}

/// Release a loaded model. This is also called automatically when the
/// `ModelHandle` is dropped, but can be invoked explicitly for clarity.
pub fn unload_model(handle: ModelHandle) {
    drop(handle);
}

/// Query the model for its metadata: description, author, input/output schema,
/// and inferred model type.
///
/// The `model_id` parameter is not known to the Swift layer; pass it in so the
/// returned `ModelInfo` can be populated with context from the Rust side. The
/// caller is also responsible for setting `file_size_bytes`.
///
/// # Errors
///
/// Returns an error if the Swift layer fails to serialize metadata or the JSON
/// cannot be parsed into the expected schema.
pub fn get_metadata(handle: &ModelHandle) -> Result<RawModelMetadata, String> {
    let ptr = unsafe { coreml_get_metadata(handle.as_ptr()) };
    let json = unsafe { read_and_free_cstring(ptr)? };
    check_error_json(&json)?;

    let raw: serde_json::Value =
        serde_json::from_str(&json).map_err(|e| format!("Failed to parse metadata JSON: {e}"))?;

    let model_type = match raw.get("model_type").and_then(|v| v.as_str()) {
        Some("Text") => ModelType::Text,
        Some("Vision") => ModelType::Vision,
        Some("Multimodal") => ModelType::Multimodal,
        Some("Audio") => ModelType::Audio,
        _ => ModelType::Unknown,
    };

    let description = raw
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());

    let author = raw
        .get("author")
        .and_then(|v| v.as_str())
        .map(|s| s.to_owned());

    let input_schema = parse_port_list(raw.get("input_schema"));
    let output_schema = parse_port_list(raw.get("output_schema"));

    Ok(RawModelMetadata {
        model_type,
        description,
        author,
        input_schema,
        output_schema,
    })
}

/// Intermediate metadata returned by the FFI layer before the caller fills in
/// fields it owns (id, name, file_size_bytes).
#[derive(Clone, Debug)]
pub struct RawModelMetadata {
    pub model_type: ModelType,
    pub description: Option<String>,
    pub author: Option<String>,
    pub input_schema: Vec<PortInfo>,
    pub output_schema: Vec<PortInfo>,
}

impl RawModelMetadata {
    /// Combine with caller-supplied fields to produce a complete `ModelInfo`.
    pub fn into_model_info(self, id: String, name: String, file_size_bytes: u64) -> ModelInfo {
        ModelInfo {
            id,
            name,
            model_type: self.model_type,
            description: self.description,
            author: self.author,
            input_schema: self.input_schema,
            output_schema: self.output_schema,
            file_size_bytes,
        }
    }
}

/// Run text inference against a loaded model.
///
/// `input` is a JSON string mapping feature names to values, e.g.
/// `{"prompt": "Hello"}`. The model's output features are returned as an
/// arbitrary JSON value.
///
/// # Errors
///
/// Returns an error if the input is not valid UTF-8, prediction fails, or the
/// output JSON is malformed.
pub fn predict_text(handle: &ModelHandle, input: &str) -> Result<serde_json::Value, String> {
    let c_input = CString::new(input).map_err(|e| format!("Invalid input string: {e}"))?;
    let ptr = unsafe { coreml_predict_text(handle.as_ptr(), c_input.as_ptr()) };
    let json = unsafe { read_and_free_cstring(ptr)? };
    check_error_json(&json)?;

    serde_json::from_str(&json).map_err(|e| format!("Failed to parse prediction JSON: {e}"))
}

/// Run image inference against a loaded model.
///
/// `image_data` is raw image bytes (JPEG, PNG, etc.) and `prompt` is an
/// optional text prompt for multimodal models.
///
/// # Errors
///
/// Returns an error if prediction fails or the output JSON is malformed.
pub fn predict_image(
    handle: &ModelHandle,
    image_data: &[u8],
    prompt: Option<&str>,
) -> Result<serde_json::Value, String> {
    let c_prompt = prompt
        .map(|p| CString::new(p).map_err(|e| format!("Invalid prompt string: {e}")))
        .transpose()?;

    let prompt_ptr = c_prompt
        .as_ref()
        .map(|c| c.as_ptr())
        .unwrap_or(std::ptr::null());

    let ptr = unsafe {
        coreml_predict_image(
            handle.as_ptr(),
            image_data.as_ptr(),
            image_data.len(),
            prompt_ptr,
        )
    };

    let json = unsafe { read_and_free_cstring(ptr)? };
    check_error_json(&json)?;

    serde_json::from_str(&json).map_err(|e| format!("Failed to parse prediction JSON: {e}"))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse the `input_schema` / `output_schema` array from the Swift metadata JSON.
fn parse_port_list(value: Option<&serde_json::Value>) -> Vec<PortInfo> {
    let Some(serde_json::Value::Array(arr)) = value else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_owned();
            let port_type = item.get("port_type")?.as_str()?.to_owned();
            let shape = item.get("shape").and_then(|s| {
                s.as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_i64()).collect::<Vec<i64>>())
            });
            Some(PortInfo {
                name,
                port_type,
                shape,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_error_json_with_error() {
        let json = r#"{"error": "something went wrong"}"#;
        let result = check_error_json(json);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "something went wrong");
    }

    #[test]
    fn test_check_error_json_without_error() {
        let json = r#"{"model_type": "Text", "input_schema": []}"#;
        assert!(check_error_json(json).is_ok());
    }

    #[test]
    fn test_check_error_json_invalid_json() {
        let json = "not json at all";
        // Should not treat invalid JSON as an error envelope.
        assert!(check_error_json(json).is_ok());
    }

    #[test]
    fn test_parse_port_list_basic() {
        let json: serde_json::Value = serde_json::json!([
            {"name": "input", "port_type": "string"},
            {"name": "image", "port_type": "image", "shape": [224, 224]},
        ]);
        let ports = parse_port_list(Some(&json));
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].name, "input");
        assert_eq!(ports[0].port_type, "string");
        assert!(ports[0].shape.is_none());
        assert_eq!(ports[1].name, "image");
        assert_eq!(ports[1].shape, Some(vec![224, 224]));
    }

    #[test]
    fn test_parse_port_list_empty() {
        let ports = parse_port_list(None);
        assert!(ports.is_empty());
    }

    #[test]
    fn test_raw_metadata_into_model_info() {
        let meta = RawModelMetadata {
            model_type: ModelType::Text,
            description: Some("A text model".into()),
            author: Some("Test".into()),
            input_schema: vec![PortInfo {
                name: "prompt".into(),
                port_type: "string".into(),
                shape: None,
            }],
            output_schema: vec![],
        };
        let info = meta.into_model_info("id-1".into(), "MyModel".into(), 1024);
        assert_eq!(info.id, "id-1");
        assert_eq!(info.name, "MyModel");
        assert_eq!(info.model_type, ModelType::Text);
        assert_eq!(info.file_size_bytes, 1024);
        assert_eq!(info.input_schema.len(), 1);
    }
}
