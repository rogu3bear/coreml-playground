/// Model discovery, lifecycle management, and directory watching.
///
/// `ModelRegistry` scans a directory for `.mlmodel` and `.mlpackage` files,
/// extracts metadata via the CoreML bridge, and keeps the catalog up to date
/// using filesystem notifications. At most one model is "active" (loaded into
/// memory for inference) at any time.
///
/// When the bridge FFI is not yet linked, the registry falls back to a set of
/// mock models so that the frontend can be developed independently.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::types::{ModelInfo, ModelType, PortInfo};

// ---------------------------------------------------------------------------
// Environment-based mock override
// ---------------------------------------------------------------------------

/// Returns `true` when the `COREML_MOCK` environment variable is set to
/// `"1"`, `"true"`, or `"yes"` (case-insensitive). This forces the registry
/// to use mock models even when real `.mlmodel` / `.mlpackage` files exist
/// on disk.
fn is_mock_mode() -> bool {
    std::env::var("COREML_MOCK")
        .map(|v| matches!(v.to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Bridge handle — opaque wrapper until the real FFI is available
// ---------------------------------------------------------------------------

/// Opaque handle to a loaded CoreML model.
///
/// The real implementation lives in `bridge::ffi`; this placeholder allows the
/// rest of the server to compile before the bridge is ready.
#[derive(Debug, Clone)]
pub struct ModelHandle {
    pub id: String,
    _path: PathBuf,
}

// ---------------------------------------------------------------------------
// Internal bookkeeping
// ---------------------------------------------------------------------------

struct RegisteredModel {
    info: ModelInfo,
    path: PathBuf,
}

// ---------------------------------------------------------------------------
// ModelRegistry
// ---------------------------------------------------------------------------

pub struct ModelRegistry {
    models_dir: PathBuf,
    models: RwLock<HashMap<String, RegisteredModel>>,
    active_model: RwLock<Option<(String, ModelHandle)>>,
}

impl ModelRegistry {
    // -- Construction -------------------------------------------------------

    /// Create a new registry and perform an initial scan of `models_dir`.
    ///
    /// If the directory does not exist it will be created so the watcher can
    /// still attach to it later.
    pub fn new(models_dir: &str) -> Self {
        let path = PathBuf::from(models_dir);
        if !path.exists() {
            let _ = std::fs::create_dir_all(&path);
        }

        let registry = Self {
            models_dir: path,
            models: RwLock::new(HashMap::new()),
            active_model: RwLock::new(None),
        };

        // When COREML_MOCK is set, skip the directory scan entirely and use
        // mock models so the frontend can be exercised without real models.
        let models = if is_mock_mode() {
            leptos::logging::log!("[registry] COREML_MOCK is set; using mock models");
            Self::mock_models()
        } else {
            // Perform blocking initial scan (called once at startup).
            Self::scan_dir(&registry.models_dir)
        };

        // Safety: we just created the lock; no contention yet.
        *registry.models.blocking_write() = models;

        registry
    }

    // -- Directory scanning -------------------------------------------------

    /// Walk `dir` for `.mlmodel` and `.mlpackage` entries, returning a map of
    /// model-id to `RegisteredModel`.
    fn scan_dir(dir: &Path) -> HashMap<String, RegisteredModel> {
        let mut map = HashMap::new();

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(err) => {
                leptos::logging::log!("[registry] cannot read models dir: {err}");
                // Fall back to mock models when the real directory is empty or
                // unreadable so the UI always has something to display.
                return Self::mock_models();
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let is_model = path
                .extension()
                .map(|ext| ext == "mlmodel" || ext == "mlpackage")
                .unwrap_or(false)
                || path.is_dir()
                    && path
                        .extension()
                        .map(|ext| ext == "mlpackage")
                        .unwrap_or(false);

            if !is_model {
                continue;
            }

            let id = Self::path_to_id(&path);
            let info = Self::extract_metadata(&id, &path);
            map.insert(id, RegisteredModel { info, path });
        }

        if map.is_empty() {
            leptos::logging::log!(
                "[registry] no models found in {}; using mocks",
                dir.display()
            );
            return Self::mock_models();
        }

        // Always include the demo echo model so users can explore the UI
        // even when real models are present.
        Self::ensure_demo_model(&mut map);

        map
    }

    /// Re-scan the models directory and replace the in-memory catalog.
    pub async fn scan(&self) {
        if is_mock_mode() {
            leptos::logging::log!("[registry] COREML_MOCK is set; using mock models");
            *self.models.write().await = Self::mock_models();
            return;
        }

        let models = {
            let dir = self.models_dir.clone();
            tokio::task::spawn_blocking(move || Self::scan_dir(&dir))
                .await
                .unwrap_or_default()
        };
        *self.models.write().await = models;
    }

    // -- Filesystem watcher -------------------------------------------------

    /// Watch the models directory for changes and re-scan when files are
    /// created or removed.  This function blocks forever — run it inside
    /// `tokio::spawn`.
    pub async fn watch(self: &Arc<Self>) {
        use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
        use std::sync::mpsc;
        use std::time::Duration;

        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

        let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
            Ok(w) => w,
            Err(err) => {
                leptos::logging::log!("[registry] failed to create watcher: {err}");
                return;
            }
        };

        if let Err(err) = watcher.watch(&self.models_dir, RecursiveMode::NonRecursive) {
            leptos::logging::log!(
                "[registry] failed to watch {}: {err}",
                self.models_dir.display()
            );
            return;
        }

        leptos::logging::log!(
            "[registry] watching {} for changes",
            self.models_dir.display()
        );

        let registry = self.clone();
        // Debounce rapid events by coalescing into 500ms windows.
        tokio::task::spawn_blocking(move || {
            let debounce = Duration::from_millis(500);
            loop {
                match rx.recv() {
                    Ok(Ok(event)) => {
                        let dominated = matches!(
                            event.kind,
                            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)
                        );
                        if !dominated {
                            continue;
                        }
                        // Drain any queued events within the debounce window.
                        while rx.recv_timeout(debounce).is_ok() {}
                        leptos::logging::log!("[registry] change detected, re-scanning");
                        let rt = tokio::runtime::Handle::current();
                        rt.block_on(registry.scan());
                    }
                    Ok(Err(err)) => {
                        leptos::logging::log!("[registry] watch error: {err}");
                    }
                    Err(_) => {
                        // Channel closed — watcher dropped.
                        break;
                    }
                }
            }
        })
        .await
        .ok();
    }

    // -- Query API ----------------------------------------------------------

    /// Return metadata for all discovered models.
    pub async fn list_models(&self) -> Vec<ModelInfo> {
        self.models
            .read()
            .await
            .values()
            .map(|rm| rm.info.clone())
            .collect()
    }

    /// Retrieve metadata for a single model without loading it.
    pub async fn get_model_info(&self, model_id: &str) -> Option<ModelInfo> {
        self.models
            .read()
            .await
            .get(model_id)
            .map(|rm| rm.info.clone())
    }

    // -- Load / unload ------------------------------------------------------

    /// Load a model into memory via the CoreML bridge and mark it as the
    /// active model. Any previously active model is unloaded first.
    ///
    /// Returns the model's metadata on success.
    pub async fn load_model(&self, model_id: &str) -> Result<ModelInfo, String> {
        let (info, path) = {
            let models = self.models.read().await;
            let rm = models
                .get(model_id)
                .ok_or_else(|| format!("model not found: {model_id}"))?;
            (rm.info.clone(), rm.path.clone())
        };

        // Unload the current model first (if any).
        self.unload_active().await;

        // Attempt to load via bridge. If the bridge is unavailable we create a
        // stand-in handle so the rest of the pipeline can proceed.
        let handle = Self::bridge_load(&path, model_id)?;

        *self.active_model.write().await = Some((model_id.to_string(), handle));

        leptos::logging::log!("[registry] loaded model {model_id} from {}", path.display());
        Ok(info)
    }

    /// Unload the currently active model, freeing CoreML resources.
    pub async fn unload_active(&self) {
        let mut active = self.active_model.write().await;
        if let Some((id, handle)) = active.take() {
            Self::bridge_unload(handle);
            leptos::logging::log!("[registry] unloaded model {id}");
        }
    }

    /// Get a clone of the active model handle for inference.
    pub async fn get_active(&self) -> Option<ModelHandle> {
        self.active_model
            .read()
            .await
            .as_ref()
            .map(|(_, h)| h.clone())
    }

    /// Get the id of the currently active model.
    pub async fn get_active_id(&self) -> Option<String> {
        self.active_model
            .read()
            .await
            .as_ref()
            .map(|(id, _)| id.clone())
    }

    // -- Helpers (bridge interaction) ---------------------------------------

    /// Attempt to load a model via the bridge FFI. Falls back to a stub handle
    /// when the real FFI functions are not yet linked.
    ///
    /// When `COREML_MOCK` is set the bridge is bypassed entirely and a mock
    /// handle is returned immediately.
    fn bridge_load(path: &Path, model_id: &str) -> Result<ModelHandle, String> {
        if is_mock_mode() {
            return Ok(ModelHandle {
                id: model_id.to_string(),
                _path: path.to_path_buf(),
            });
        }

        // TODO: Replace with real bridge call once ffi.rs is ready:
        //   bridge::ffi::load_model(path.to_str().unwrap_or_default())
        let _ = path;
        Ok(ModelHandle {
            id: model_id.to_string(),
            _path: path.to_path_buf(),
        })
    }

    /// Unload a model handle via the bridge FFI.
    fn bridge_unload(_handle: ModelHandle) {
        // TODO: Replace with real bridge call once ffi.rs is ready:
        //   bridge::ffi::unload_model(handle)
    }

    // -- Metadata extraction ------------------------------------------------

    /// Derive a stable, deterministic ID from a file path by hashing it.
    fn path_to_id(path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Extract (or synthesise) metadata for a model file.
    fn extract_metadata(id: &str, path: &Path) -> ModelInfo {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let file_size_bytes = if path.is_dir() {
            // `.mlpackage` is a directory — sum contents.
            walkdir_size(path)
        } else {
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        };

        // TODO: Use bridge::ffi::get_metadata once available to populate
        // real model_type, input_schema, output_schema, description, author.
        ModelInfo {
            id: id.to_string(),
            name,
            model_type: ModelType::Unknown,
            description: None,
            author: None,
            input_schema: Vec::new(),
            output_schema: Vec::new(),
            file_size_bytes,
        }
    }

    // -- Demo model (CPO-9) -------------------------------------------------

    /// A built-in demo model that is always available when no real models are
    /// found. It echoes user input with some formatting so new users get a
    /// zero-friction first experience without needing real CoreML models.
    pub fn create_demo_model() -> ModelInfo {
        ModelInfo {
            id: "demo-echo".to_string(),
            name: "Echo Demo".to_string(),
            model_type: ModelType::Text,
            description: Some(
                "Built-in demo model that echoes your input. Load a real \
                 CoreML model to run actual inference."
                    .to_string(),
            ),
            author: Some("CoreML Playground".to_string()),
            input_schema: vec![PortInfo {
                name: "text".into(),
                port_type: "String".into(),
                shape: None,
            }],
            output_schema: vec![PortInfo {
                name: "echo".into(),
                port_type: "String".into(),
                shape: None,
            }],
            file_size_bytes: 0,
        }
    }

    /// Ensure the demo model is present in a models map.
    ///
    /// Called after scanning / building mock models so the echo demo is
    /// always reachable by the inference pipeline.
    fn ensure_demo_model(map: &mut HashMap<String, RegisteredModel>) {
        map.entry("demo-echo".to_string())
            .or_insert_with(|| RegisteredModel {
                info: Self::create_demo_model(),
                path: PathBuf::from("/builtin/demo-echo"),
            });
    }

    // -- Mock data for development ------------------------------------------

    /// Return a set of realistic fake models so the frontend can be developed
    /// and tested without any `.mlmodel` files on disk.
    fn mock_models() -> HashMap<String, RegisteredModel> {
        let mut map = HashMap::new();

        // Always include the demo model for zero-friction first experience.
        Self::ensure_demo_model(&mut map);

        // 1. Vision classifier
        map.insert(
            "mock-mobilenetv2".into(),
            RegisteredModel {
                info: ModelInfo {
                    id: "mock-mobilenetv2".into(),
                    name: "MobileNetV2".into(),
                    model_type: ModelType::Vision,
                    description: Some(
                        "Image classification model trained on ImageNet. Classifies images \
                         into 1000 categories with high accuracy and low latency."
                            .into(),
                    ),
                    author: Some("Apple".into()),
                    input_schema: vec![PortInfo {
                        name: "image".into(),
                        port_type: "Image".into(),
                        shape: Some(vec![1, 3, 224, 224]),
                    }],
                    output_schema: vec![
                        PortInfo {
                            name: "classLabel".into(),
                            port_type: "String".into(),
                            shape: None,
                        },
                        PortInfo {
                            name: "classLabelProbs".into(),
                            port_type: "Dictionary".into(),
                            shape: None,
                        },
                    ],
                    file_size_bytes: 14_232_576,
                },
                path: PathBuf::from("/mock/MobileNetV2.mlmodel"),
            },
        );

        // 2. Text sentiment analyser
        map.insert(
            "mock-textsentiment".into(),
            RegisteredModel {
                info: ModelInfo {
                    id: "mock-textsentiment".into(),
                    name: "TextSentiment".into(),
                    model_type: ModelType::Text,
                    description: Some(
                        "Sentiment analysis model for English text. Returns a positive/negative \
                         label and confidence score."
                            .into(),
                    ),
                    author: Some("Create ML".into()),
                    input_schema: vec![PortInfo {
                        name: "text".into(),
                        port_type: "String".into(),
                        shape: None,
                    }],
                    output_schema: vec![
                        PortInfo {
                            name: "label".into(),
                            port_type: "String".into(),
                            shape: None,
                        },
                        PortInfo {
                            name: "score".into(),
                            port_type: "Float64".into(),
                            shape: None,
                        },
                    ],
                    file_size_bytes: 2_457_600,
                },
                path: PathBuf::from("/mock/TextSentiment.mlmodel"),
            },
        );

        // 3. Audio / speech model
        map.insert(
            "mock-whispertiny".into(),
            RegisteredModel {
                info: ModelInfo {
                    id: "mock-whispertiny".into(),
                    name: "WhisperTiny".into(),
                    model_type: ModelType::Audio,
                    description: Some(
                        "Tiny variant of Whisper for speech-to-text. Fast transcription with \
                         reasonable accuracy for short audio clips."
                            .into(),
                    ),
                    author: Some("OpenAI (converted)".into()),
                    input_schema: vec![PortInfo {
                        name: "audio".into(),
                        port_type: "MultiArray".into(),
                        shape: Some(vec![1, 80, 3000]),
                    }],
                    output_schema: vec![PortInfo {
                        name: "text".into(),
                        port_type: "String".into(),
                        shape: None,
                    }],
                    file_size_bytes: 78_643_200,
                },
                path: PathBuf::from("/mock/WhisperTiny.mlpackage"),
            },
        );

        map
    }
}

// ---------------------------------------------------------------------------
// Utility: recursive directory size
// ---------------------------------------------------------------------------

/// Sum the sizes of all regular files under `dir`.
fn walkdir_size(dir: &Path) -> u64 {
    fn walk(path: &Path, total: &mut u64) {
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, total);
                } else if let Ok(meta) = p.metadata() {
                    *total += meta.len();
                }
            }
        }
    }
    let mut size = 0u64;
    walk(dir, &mut size);
    size
}
