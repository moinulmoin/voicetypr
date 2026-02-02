#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParakeetCommand {
    LoadModel {
        model_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        force_download: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        local_path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_dir: Option<String>,
        #[serde(default = "default_precision")]
        precision: String,
        #[serde(default = "default_attention")]
        attention: String,
        #[serde(default = "default_local_context")]
        local_attention_context: i32,
        #[serde(skip_serializing_if = "Option::is_none")]
        chunk_duration: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        overlap_duration: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        eager_unload: Option<bool>,
    },
    UnloadModel {},
    Transcribe {
        audio_path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        #[serde(default)]
        translate_to_english: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        use_word_timestamps: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        chunk_duration: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        overlap_duration: Option<f32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        attention: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        local_attention_context: Option<i32>,
    },
    Status {},
    DeleteModel {
        #[serde(skip_serializing_if = "Option::is_none")]
        model_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        model_version: Option<String>,
    },
    Shutdown {},
}

fn default_precision() -> String {
    "bf16".to_string()
}

fn default_attention() -> String {
    "full".to_string()
}

fn default_local_context() -> i32 {
    256
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParakeetResponse {
    #[serde(rename_all = "camelCase")]
    Ok {
        command: String,
        #[serde(default)]
        payload: HashMap<String, Value>,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        code: String,
        message: String,
        #[serde(default)]
        details: Option<Value>,
    },
    #[serde(rename_all = "camelCase")]
    Status {
        loaded_model: Option<String>,
        #[serde(default)]
        model_version: Option<String>,
        model_path: Option<String>,
        precision: Option<String>,
        attention: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Transcription {
        text: String,
        #[serde(default)]
        segments: Vec<ParakeetSegment>,
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        duration: Option<f32>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParakeetSegment {
    pub text: String,
    #[serde(default)]
    pub start: Option<f32>,
    #[serde(default)]
    pub end: Option<f32>,
    #[serde(default)]
    pub tokens: Option<Vec<Value>>,
}
