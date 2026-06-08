#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct ParakeetVocabularyTerm {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

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
        #[serde(skip_serializing_if = "Option::is_none")]
        custom_vocabulary: Option<Vec<ParakeetVocabularyTerm>>,
    },
    Diarize {
        audio_path: String,
    },
    Status {},
    DownloadCtcModels {},
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
        #[serde(default)]
        custom_vocabulary_supported: bool,
        #[serde(default)]
        custom_vocabulary_ready: bool,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        progress: f32,
        #[serde(default)]
        phase: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Diarization {
        #[serde(default)]
        segments: Vec<ParakeetSpeakerSegment>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct ParakeetSpeakerSegment {
    #[serde(rename = "speakerId")]
    pub speaker_id: String,
    pub start: f32,
    pub end: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcribe_command_serializes_custom_vocabulary_when_present() {
        let command = ParakeetCommand::Transcribe {
            audio_path: "/tmp/audio.wav".to_string(),
            language: Some("en".to_string()),
            translate_to_english: false,
            prompt: None,
            use_word_timestamps: Some(true),
            chunk_duration: None,
            overlap_duration: None,
            attention: None,
            local_attention_context: None,
            custom_vocabulary: Some(vec![
                ParakeetVocabularyTerm {
                    text: "VoiceTypr".to_string(),
                    aliases: vec!["voice typer".to_string()],
                },
                ParakeetVocabularyTerm {
                    text: "Tauri".to_string(),
                    aliases: Vec::new(),
                },
            ]),
        };

        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["type"], "transcribe");
        assert_eq!(value["custom_vocabulary"][0]["text"], "VoiceTypr");
        assert_eq!(value["custom_vocabulary"][0]["aliases"][0], "voice typer");
        assert!(value["custom_vocabulary"][1].get("aliases").is_none());
    }

    #[test]
    fn transcribe_command_omits_empty_custom_vocabulary() {
        let command = ParakeetCommand::Transcribe {
            audio_path: "/tmp/audio.wav".to_string(),
            language: None,
            translate_to_english: false,
            prompt: None,
            use_word_timestamps: Some(true),
            chunk_duration: None,
            overlap_duration: None,
            attention: None,
            local_attention_context: None,
            custom_vocabulary: None,
        };

        let value = serde_json::to_value(command).unwrap();
        assert!(value.get("custom_vocabulary").is_none());
    }

    #[test]
    fn ctc_download_command_serializes_snake_case_type() {
        let value = serde_json::to_value(ParakeetCommand::DownloadCtcModels {}).unwrap();
        assert_eq!(value["type"], "download_ctc_models");
    }

    #[test]
    fn status_response_defaults_custom_vocabulary_flags() {
        let response: ParakeetResponse = serde_json::from_value(serde_json::json!({
            "type": "status",
            "loadedModel": null,
            "modelPath": null,
            "precision": null,
            "attention": null
        }))
        .unwrap();

        match response {
            ParakeetResponse::Status {
                custom_vocabulary_supported,
                custom_vocabulary_ready,
                ..
            } => {
                assert!(!custom_vocabulary_supported);
                assert!(!custom_vocabulary_ready);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn status_response_decodes_custom_vocabulary_flags() {
        let response: ParakeetResponse = serde_json::from_value(serde_json::json!({
            "type": "status",
            "loadedModel": null,
            "modelPath": null,
            "precision": null,
            "attention": null,
            "customVocabularySupported": true,
            "customVocabularyReady": true
        }))
        .unwrap();

        match response {
            ParakeetResponse::Status {
                custom_vocabulary_supported,
                custom_vocabulary_ready,
                ..
            } => {
                assert!(custom_vocabulary_supported);
                assert!(custom_vocabulary_ready);
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }
}
