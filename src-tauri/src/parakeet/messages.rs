#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

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

pub const SHORT_REQUEST_TIMEOUT_SECS: u64 = 30;
pub const LOAD_MODEL_TIMEOUT_SECS: u64 = 300;
pub const DOWNLOAD_MODEL_TIMEOUT_SECS: u64 = 60 * 60;
pub const TRANSCRIBE_TIMEOUT_SECS: u64 = 180;
pub const MAX_TRANSCRIBE_TIMEOUT_SECS: u64 = 30 * 60;

impl ParakeetCommand {
    pub fn operation_name(&self) -> &'static str {
        match self {
            Self::LoadModel { .. } => "load_model",
            Self::UnloadModel { .. } => "unload_model",
            Self::Transcribe { .. } => "transcribe",
            Self::Status { .. } => "status",
            Self::DeleteModel { .. } => "delete_model",
            Self::Shutdown { .. } => "shutdown",
        }
    }

    pub fn request_timeout_secs(&self) -> u64 {
        match self {
            Self::LoadModel { force_download, .. } if force_download.unwrap_or(false) => {
                DOWNLOAD_MODEL_TIMEOUT_SECS
            }
            Self::LoadModel { .. } => LOAD_MODEL_TIMEOUT_SECS,
            Self::Transcribe { audio_path, .. } => transcribe_timeout_secs(audio_path),
            Self::Status { .. }
            | Self::Shutdown { .. }
            | Self::DeleteModel { .. }
            | Self::UnloadModel { .. } => SHORT_REQUEST_TIMEOUT_SECS,
        }
    }
}

fn transcribe_timeout_secs(audio_path: &str) -> u64 {
    wav_duration_seconds(Path::new(audio_path))
        .map(transcribe_timeout_secs_for_duration)
        .unwrap_or(TRANSCRIBE_TIMEOUT_SECS)
}

fn transcribe_timeout_secs_for_duration(duration_secs: f64) -> u64 {
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return TRANSCRIBE_TIMEOUT_SECS;
    }

    (duration_secs.ceil() as u64)
        .saturating_mul(4)
        .saturating_add(60)
        .clamp(TRANSCRIBE_TIMEOUT_SECS, MAX_TRANSCRIBE_TIMEOUT_SECS)
}

fn wav_duration_seconds(audio_path: &Path) -> Option<f64> {
    let reader = hound::WavReader::open(audio_path).ok()?;
    let spec = reader.spec();
    if spec.sample_rate == 0 || spec.channels == 0 {
        return None;
    }

    let frames = reader.duration() as f64;
    Some(frames / f64::from(spec.sample_rate))
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

#[cfg(test)]
mod tests {
    use super::{
        ParakeetCommand, DOWNLOAD_MODEL_TIMEOUT_SECS, LOAD_MODEL_TIMEOUT_SECS,
        SHORT_REQUEST_TIMEOUT_SECS, TRANSCRIBE_TIMEOUT_SECS,
    };

    #[test]
    fn operation_name_maps_each_command_variant() {
        assert_eq!(ParakeetCommand::Status {}.operation_name(), "status");
        assert_eq!(ParakeetCommand::Shutdown {}.operation_name(), "shutdown");
        assert_eq!(
            ParakeetCommand::UnloadModel {}.operation_name(),
            "unload_model"
        );
        assert_eq!(
            ParakeetCommand::DeleteModel {
                model_id: None,
                model_version: None,
            }
            .operation_name(),
            "delete_model"
        );
        assert_eq!(
            ParakeetCommand::LoadModel {
                model_id: "parakeet-tdt-0.6b-v2".to_string(),
                model_version: None,
                force_download: None,
                local_path: None,
                cache_dir: None,
                precision: "bf16".to_string(),
                attention: "full".to_string(),
                local_attention_context: 256,
                chunk_duration: None,
                overlap_duration: None,
                eager_unload: None,
            }
            .operation_name(),
            "load_model"
        );
        assert_eq!(
            ParakeetCommand::Transcribe {
                audio_path: "/tmp/audio.wav".to_string(),
                language: None,
                translate_to_english: false,
                prompt: None,
                use_word_timestamps: None,
                chunk_duration: None,
                overlap_duration: None,
                attention: None,
                local_attention_context: None,
            }
            .operation_name(),
            "transcribe"
        );
    }

    #[test]
    fn request_timeout_secs_uses_short_timeout_for_control_commands() {
        assert_eq!(
            ParakeetCommand::Status {}.request_timeout_secs(),
            SHORT_REQUEST_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::Shutdown {}.request_timeout_secs(),
            SHORT_REQUEST_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::UnloadModel {}.request_timeout_secs(),
            SHORT_REQUEST_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::DeleteModel {
                model_id: None,
                model_version: None,
            }
            .request_timeout_secs(),
            SHORT_REQUEST_TIMEOUT_SECS
        );
    }

    #[test]
    fn request_timeout_secs_distinguishes_cached_load_from_download() {
        assert_eq!(
            ParakeetCommand::LoadModel {
                model_id: "parakeet-tdt-0.6b-v2".to_string(),
                model_version: None,
                force_download: None,
                local_path: None,
                cache_dir: None,
                precision: "bf16".to_string(),
                attention: "full".to_string(),
                local_attention_context: 256,
                chunk_duration: None,
                overlap_duration: None,
                eager_unload: None,
            }
            .request_timeout_secs(),
            LOAD_MODEL_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::LoadModel {
                model_id: "parakeet-tdt-0.6b-v2".to_string(),
                model_version: None,
                force_download: Some(true),
                local_path: None,
                cache_dir: None,
                precision: "bf16".to_string(),
                attention: "full".to_string(),
                local_attention_context: 256,
                chunk_duration: None,
                overlap_duration: None,
                eager_unload: None,
            }
            .request_timeout_secs(),
            DOWNLOAD_MODEL_TIMEOUT_SECS
        );
        assert_eq!(
            ParakeetCommand::Transcribe {
                audio_path: "/tmp/audio.wav".to_string(),
                language: None,
                translate_to_english: false,
                prompt: None,
                use_word_timestamps: None,
                chunk_duration: None,
                overlap_duration: None,
                attention: None,
                local_attention_context: None,
            }
            .request_timeout_secs(),
            TRANSCRIBE_TIMEOUT_SECS
        );
    }

    #[test]
    fn request_timeout_secs_uses_channel_independent_wav_duration() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..(31 * 100 * 2) {
            writer.write_sample::<i16>(0).unwrap();
        }
        writer.finalize().unwrap();

        let timeout = ParakeetCommand::Transcribe {
            audio_path: path.to_string_lossy().to_string(),
            language: None,
            translate_to_english: false,
            prompt: None,
            use_word_timestamps: None,
            chunk_duration: None,
            overlap_duration: None,
            attention: None,
            local_attention_context: None,
        }
        .request_timeout_secs();

        assert_eq!(timeout, 184);
    }
}
