use serde::{Deserialize, Serialize};

// -- Context signal newtypes --
//
// Leptos `provide_context`/`use_context` resolves by **type alone**. When
// multiple signals share the same type (e.g. two `WriteSignal<bool>`s), the
// last one provided shadows all earlier ones. These newtype wrappers give each
// signal a unique type so context resolution is unambiguous.

#[cfg(not(feature = "ssr"))]
use leptos::prelude::*;
#[cfg(feature = "ssr")]
use leptos::prelude::*;

// -- bool signals (show_introspection vs model_picker_open) --
#[derive(Clone, Copy)]
pub struct ShowIntrospection(pub ReadSignal<bool>);
#[derive(Clone, Copy)]
pub struct SetShowIntrospection(pub WriteSignal<bool>);
#[derive(Clone, Copy)]
pub struct ModelPickerOpen(pub ReadSignal<bool>);
#[derive(Clone, Copy)]
pub struct SetModelPickerOpen(pub WriteSignal<bool>);

// -- u64 signals (session_version vs shortcut_new_session) --
#[derive(Clone, Copy)]
pub struct SessionVersion(pub ReadSignal<u64>);
#[derive(Clone, Copy)]
pub struct SetSessionVersion(pub WriteSignal<u64>);
#[derive(Clone, Copy)]
pub struct ShortcutNewSession(pub ReadSignal<u64>);
#[derive(Clone, Copy)]
pub struct SetShortcutNewSession(pub WriteSignal<u64>);

// -- Missing signal: last inference timing --
#[derive(Clone, Copy)]
pub struct LastInferenceMs(pub ReadSignal<Option<u64>>);
#[derive(Clone, Copy)]
pub struct SetLastInferenceMs(pub WriteSignal<Option<u64>>);

// -- Comparison view visibility --
#[derive(Clone, Copy)]
pub struct ShowComparison(pub ReadSignal<bool>);
#[derive(Clone, Copy)]
pub struct SetShowComparison(pub WriteSignal<bool>);

// -- Model metadata --

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub model_type: ModelType,
    pub description: Option<String>,
    pub author: Option<String>,
    pub input_schema: Vec<PortInfo>,
    pub output_schema: Vec<PortInfo>,
    pub file_size_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ModelType {
    Text,
    Vision,
    Multimodal,
    Audio,
    Unknown,
}

impl ModelType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Vision => "Vision",
            Self::Multimodal => "Multimodal",
            Self::Audio => "Audio",
            Self::Unknown => "Unknown",
        }
    }

    pub fn accepts_image(&self) -> bool {
        matches!(self, Self::Vision | Self::Multimodal)
    }

    pub fn accepts_text(&self) -> bool {
        matches!(self, Self::Text | Self::Multimodal)
    }

    pub fn is_chat_compatible(&self) -> bool {
        matches!(self, Self::Text | Self::Vision | Self::Multimodal)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PortInfo {
    pub name: String,
    pub port_type: String,
    pub shape: Option<Vec<i64>>,
}

impl PortInfo {
    pub fn humanize(&self) -> String {
        let pt = self.port_type.to_lowercase();

        if let Some(ref shape) = self.shape {
            // [1,3,H,W] color image
            if pt.contains("image") && shape.len() == 4 && shape[0] == 1 && shape[1] == 3 {
                return format!("Color image ({}x{} pixels)", shape[2], shape[3]);
            }
            // [1,1,H,W] grayscale image
            if pt.contains("image") && shape.len() == 4 && shape[0] == 1 && shape[1] == 1 {
                return format!("Grayscale image ({}x{} pixels)", shape[2], shape[3]);
            }
            // float32/float64 + [1,N]
            if (pt == "float32" || pt == "float64") && shape.len() == 2 && shape[0] == 1 {
                let n = shape[1];
                if n == 1000 {
                    return "1,000 category scores".to_string();
                }
                return format!("{n} numbers");
            }
            // int32/int64 + [1,N]
            if (pt == "int32" || pt == "int64") && shape.len() == 2 && shape[0] == 1 {
                return format!("{} integers (token IDs)", shape[1]);
            }
            // 4-dim generic [B,C,H,W]
            if shape.len() == 4 {
                return format!(
                    "Batch of {}-channel {}x{} feature maps",
                    shape[1], shape[2], shape[3]
                );
            }
        }

        if pt.contains("image") {
            return "Image".to_string();
        }
        if pt == "string" {
            return "Text".to_string();
        }
        if pt.contains("dictionary") {
            return "Key-value pairs".to_string();
        }

        // Fallback
        match &self.shape {
            Some(s) => format!("{} (shape {:?})", self.port_type, s),
            None => self.port_type.clone(),
        }
    }

    pub fn humanize_short(&self) -> String {
        let pt = self.port_type.to_lowercase();

        if let Some(ref shape) = self.shape {
            if pt.contains("image") && shape.len() == 4 && shape[0] == 1 {
                return format!("{}x{} image", shape[2], shape[3]);
            }
            if (pt == "float32" || pt == "float64") && shape.len() == 2 && shape[0] == 1 {
                let n = shape[1];
                if n == 1000 {
                    return "1000 scores".to_string();
                }
                return format!("{n} scores");
            }
            if (pt == "int32" || pt == "int64") && shape.len() == 2 && shape[0] == 1 {
                return format!("{} tokens", shape[1]);
            }
            if shape.len() == 4 {
                return format!("{}x{} features", shape[2], shape[3]);
            }
        }

        if pt == "string" {
            return "text".to_string();
        }
        if pt.contains("dictionary") {
            return "dict".to_string();
        }

        self.port_type.clone()
    }
}

// -- Chat messages --

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: MessageContent,
    pub timestamp: i64,
    pub inference_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Model,
    System,
}

/// A single item in a batch inference result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchItem {
    pub image_base64: String,
    pub mime_type: String,
    pub output: serde_json::Value,
    pub inference_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Image {
        data_base64: String,
        mime_type: String,
        caption: Option<String>,
    },
    ModelOutput(serde_json::Value),
    Streaming {
        partial: String,
        done: bool,
    },
    Batch(Vec<BatchItem>),
}

impl MessageContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            Self::Streaming { partial, .. } => Some(partial),
            _ => None,
        }
    }

    pub fn preview(&self, max_len: usize) -> String {
        match self {
            Self::Text(s) => truncate(s, max_len),
            Self::Image { caption, .. } => {
                caption.as_deref().map(|c| truncate(c, max_len)).unwrap_or_else(|| "[image]".into())
            }
            Self::ModelOutput(_) => "[model output]".into(),
            Self::Streaming { partial, .. } => truncate(partial, max_len),
            Self::Batch(items) => format!("Batch: {} images", items.len()),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.min(s.len())])
    }
}

// -- Sessions --

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub model_id: String,
    pub model_name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub preview: String,
    pub message_count: usize,
}

// -- Inference request/response --

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub model_id: String,
    pub session_id: String,
    pub input: InferenceInput,
}

/// A single image in a batch inference request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchImageInput {
    pub data_base64: String,
    pub mime_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum InferenceInput {
    Text(String),
    Image {
        data_base64: String,
        mime_type: String,
        prompt: Option<String>,
    },
    BatchImages {
        images: Vec<BatchImageInput>,
        prompt: Option<String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub output: serde_json::Value,
    pub inference_ms: u64,
}

// -- Latency helpers --

pub fn describe_latency(ms: u64) -> &'static str {
    match ms {
        0..=49 => "instant",
        50..=199 => "faster than a blink",
        200..=499 => "quick",
        500..=999 => "a moment",
        1000..=2999 => "working on it",
        _ => "heavy lifting",
    }
}

pub fn latency_display(ms: u64) -> String {
    format!("{}ms \u{2014} {}", ms, describe_latency(ms))
}

// -- WebSocket protocol --

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WsMessage {
    Token(String),
    Done { inference_ms: u64 },
    Error(String),
}
