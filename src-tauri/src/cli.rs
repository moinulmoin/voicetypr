use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use serde_json::json;
use tauri::async_runtime::{Mutex as AsyncMutex, RwLock as AsyncRwLock};
use tauri::Manager;
use tauri_plugin_store::StoreExt;

use crate::audio::recorder::AudioRecorder;
use crate::commands::ai::{cache_ai_api_key, CacheApiKeyArgs};
use crate::commands::audio::transcribe_audio_file_for_cli;
use crate::commands::keyring::keyring_get;
use crate::commands::license::check_license_status;
use crate::commands::model::get_model_status;
use crate::commands::remote::load_remote_settings;
use crate::commands::settings::get_settings;
use crate::parakeet::ParakeetManager;
use crate::remote::client::{
    self, RemoteServerConnection, TranscriptionRequest, TranscriptionSource,
};
use crate::remote::lifecycle::RemoteServerManager;
use crate::state::AppState;
use crate::transcription::{
    TranscriptionJob, TranscriptionResult, TranscriptionSource as ContractSource,
};
use crate::whisper::cache::TranscriberCache;
use crate::whisper::manager::WhisperManager;

#[derive(Parser, Debug)]
#[command(name = "voicetypr", version, about = "VoiceTypr CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    Status(StatusArgs),
    Models(OutputArgs),
    Transcribe(TranscribeArgs),
    Record(RecordArgs),
}

#[derive(Args, Debug, Clone)]
struct OutputArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct StatusArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct TranscribeArgs {
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    engine: Option<String>,
    #[arg(long)]
    server: Option<String>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct RecordArgs {
    #[arg(long)]
    until_silence: bool,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    engine: Option<String>,
    #[arg(long)]
    server: Option<String>,
    #[arg(long)]
    password: Option<String>,
    #[arg(long)]
    json: bool,
}

pub fn maybe_run_from_env_with_context(
    context: tauri::Context<tauri::Wry>,
) -> Result<bool, Box<dyn Error>> {
    let first_arg = env::args().nth(1);
    let Some(first_arg) = first_arg.as_deref() else {
        return Ok(false);
    };
    if !matches!(first_arg, "status" | "models" | "transcribe" | "record") {
        return Ok(false);
    }

    let cli = Cli::try_parse()?;
    let Some(command) = cli.command else {
        return Ok(false);
    };

    tauri::async_runtime::block_on(async move {
        let app = build_cli_app(context).await?;
        let app_handle = app.handle().clone();
        warm_ai_key_cache(&app_handle).await?;
        let _ = check_license_status(app_handle.clone()).await;

        match command {
            CliCommand::Status(args) => run_status(&app_handle, args).await?,
            CliCommand::Models(args) => run_models(&app_handle, args).await?,
            CliCommand::Transcribe(args) => run_transcribe(&app_handle, args).await?,
            CliCommand::Record(args) => run_record(&app_handle, args).await?,
        }

        Ok::<(), Box<dyn Error>>(())
    })?;

    Ok(true)
}

async fn build_cli_app(
    context: tauri::Context<tauri::Wry>,
) -> Result<tauri::App<tauri::Wry>, Box<dyn Error>> {
    crate::secure_store::initialize_encryption_key()?;

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .build(context)?;

    let models_dir = app.path().app_data_dir()?.join("models");
    std::fs::create_dir_all(&models_dir)?;
    let parakeet_dir = models_dir.join("parakeet");
    std::fs::create_dir_all(&parakeet_dir)?;

    app.manage(AsyncRwLock::new(WhisperManager::new(models_dir.clone())));
    app.manage(ParakeetManager::new(parakeet_dir));
    app.manage(AsyncMutex::new(TranscriberCache::new()));
    app.manage(AppState::new());
    app.manage(AsyncMutex::new(RemoteServerManager::new()));
    app.manage(AsyncMutex::new(load_remote_settings(app.handle())));

    Ok(app)
}

async fn warm_ai_key_cache(app: &tauri::AppHandle) -> Result<(), Box<dyn Error>> {
    for provider in ["openai", "anthropic", "gemini", "custom"] {
        let key = format!("ai_api_key_{}", provider);
        if let Some(api_key) = keyring_get(app.clone(), key)? {
            cache_ai_api_key(
                app.clone(),
                CacheApiKeyArgs {
                    provider: provider.to_string(),
                    api_key,
                },
            )
            .await?;
        }
    }
    Ok(())
}

async fn run_status(app: &tauri::AppHandle, _args: StatusArgs) -> Result<(), Box<dyn Error>> {
    let settings = get_settings(app.clone()).await?;
    let availability = crate::recognition_availability_snapshot(app).await;
    let payload = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "settings": {
            "current_model": settings.current_model,
            "current_model_engine": settings.current_model_engine,
            "speech_language": settings.speech_language,
            "transcription_task": settings.transcription_task,
            "final_text_language": settings.final_text_language,
        },
        "availability": availability,
    });

    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

async fn run_models(app: &tauri::AppHandle, _args: OutputArgs) -> Result<(), Box<dyn Error>> {
    let response = get_model_status(
        app.state::<AsyncRwLock<WhisperManager>>(),
        app.state::<ParakeetManager>(),
        app.clone(),
    )
    .await?;

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

async fn run_transcribe(
    app: &tauri::AppHandle,
    args: TranscribeArgs,
) -> Result<(), Box<dyn Error>> {
    if args.server.is_none() {
        check_license_status(app.clone()).await?;
    }
    let payload = if let Some(server) = args.server.as_deref() {
        transcribe_via_remote(app, &args.file, server, args.password.clone()).await?
    } else {
        let settings = get_settings(app.clone()).await?;
        let model = args
            .model
            .clone()
            .or_else(|| {
                (!settings.current_model.is_empty()).then_some(settings.current_model.clone())
            })
            .ok_or_else(|| "No model specified and no current model is selected".to_string())?;
        let engine = args.engine.clone().or_else(|| {
            (!settings.current_model_engine.is_empty())
                .then_some(settings.current_model_engine.clone())
        });
        let text = transcribe_audio_file_for_cli(
            app.clone(),
            args.file.to_string_lossy().to_string(),
            model.clone(),
            engine.clone(),
        )
        .await?;
        json!({ "text": text, "model": model, "engine": engine })
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", payload["text"].as_str().unwrap_or_default());
    }
    Ok(())
}

async fn run_record(app: &tauri::AppHandle, args: RecordArgs) -> Result<(), Box<dyn Error>> {
    if !args.until_silence {
        return Err("record currently requires --until-silence".into());
    }

    check_license_status(app.clone()).await?;
    let settings = get_settings(app.clone()).await?;
    let recordings_dir = app.path().app_data_dir()?.join("recordings");
    std::fs::create_dir_all(&recordings_dir)?;
    let output_path = recordings_dir.join(format!(
        "cli-recording-{}.wav",
        chrono::Utc::now().format("%Y%m%d%H%M%S")
    ));

    let mut recorder = AudioRecorder::new();
    recorder.start_recording(
        output_path
            .to_str()
            .ok_or_else(|| "Invalid recording path".to_string())?,
        settings.selected_microphone.clone(),
    )?;
    eprintln!("Recording… stop by silence.");
    let stop_message = recorder.wait_for_recording_end()?;

    let payload = if let Some(server) = args.server.as_deref() {
        let mut payload =
            transcribe_via_remote(app, &output_path, server, args.password.clone()).await?;
        payload["stop_reason"] = json!(stop_message);
        payload
    } else {
        let model = args
            .model
            .clone()
            .or_else(|| {
                (!settings.current_model.is_empty()).then_some(settings.current_model.clone())
            })
            .ok_or_else(|| "No model specified and no current model is selected".to_string())?;
        let engine = args.engine.clone().or_else(|| {
            (!settings.current_model_engine.is_empty())
                .then_some(settings.current_model_engine.clone())
        });
        let text = transcribe_audio_file_for_cli(
            app.clone(),
            output_path.to_string_lossy().to_string(),
            model.clone(),
            engine.clone(),
        )
        .await?;
        json!({ "text": text, "model": model, "engine": engine, "stop_reason": stop_message })
    };

    let _ = std::fs::remove_file(&output_path);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", payload["text"].as_str().unwrap_or_default());
    }
    Ok(())
}

struct RemoteNormalizedAudio {
    path: PathBuf,
}

impl RemoteNormalizedAudio {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RemoteNormalizedAudio {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "Failed to remove CLI normalized temp file {:?}: {}",
                    self.path,
                    error
                );
            }
        }
    }
}

async fn normalize_audio_for_remote(
    app: &tauri::AppHandle,
    file: &Path,
) -> Result<RemoteNormalizedAudio, Box<dyn Error>> {
    let recordings_dir = app.path().app_data_dir()?.join("recordings");
    std::fs::create_dir_all(&recordings_dir)?;

    let output_path = recordings_dir.join(format!(
        "cli-remote-normalized-{}.wav",
        chrono::Utc::now().format("%Y%m%d%H%M%S%3f")
    ));
    crate::ffmpeg::normalize_streaming(app, file, &output_path)
        .await
        .map_err(std::io::Error::other)?;

    Ok(RemoteNormalizedAudio { path: output_path })
}

async fn transcribe_via_remote(
    app: &tauri::AppHandle,
    file: &Path,
    server: &str,
    password: Option<String>,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let settings = get_settings(app.clone()).await?;
    let (host, port) = parse_server(server)?;
    let normalized_file = normalize_audio_for_remote(app, file).await?;
    let audio_data = std::fs::read(normalized_file.path())?;
    let request = TranscriptionRequest::new(audio_data, TranscriptionSource::Upload)
        .with_language_and_task(
            Some(settings.speech_language.clone()),
            Some(settings.transcription_task.clone()),
        );
    let timeout_ms = client::timeout_ms_for_wav_file(
        normalized_file.path().to_string_lossy().as_ref(),
        TranscriptionSource::Upload,
    );
    let connection = RemoteServerConnection::new(host, port, password);
    let response = client::transcribe_audio(&connection, request, timeout_ms).await?;

    let job = TranscriptionJob::from_legacy_settings(
        ContractSource::RemoteServer,
        "remote",
        response.model.clone(),
        Some(settings.speech_language.clone()),
        settings.transcription_task
            == crate::commands::settings::TRANSCRIPTION_TASK_TRANSLATE_TO_ENGLISH,
    );
    let transcription = TranscriptionResult::new(&job, response.text)
        .with_transcript_language(response.transcript_language)
        .with_processing_duration_ms(Some(response.duration_ms));
    let ai_enabled = app
        .store("settings")?
        .get("ai_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let writing =
        crate::writing::process_transcription(app.clone(), transcription, ai_enabled).await?;

    Ok(json!({
        "text": writing.final_text,
        "output_language": writing.output_language,
        "mode": writing.mode,
        "applied_operations": writing.applied_operations,
        "warnings": writing.warnings,
        "model": response.model,
        "duration_ms": response.duration_ms,
    }))
}

fn parse_server(value: &str) -> Result<(String, u16), Box<dyn Error>> {
    let (host, port) = value
        .rsplit_once(':')
        .ok_or_else(|| "Server must be host:port".to_string())?;
    let port = port.parse::<u16>()?;
    Ok((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_server_requires_port() {
        assert!(parse_server("localhost").is_err());
        let parsed = parse_server("localhost:47842").unwrap();
        assert_eq!(parsed.0, "localhost");
        assert_eq!(parsed.1, 47842);
    }

    #[test]
    fn cli_parses_transcribe_command() {
        let cli = Cli::try_parse_from([
            "voicetypr",
            "transcribe",
            "--file",
            "sample.wav",
            "--server",
            "localhost:47842",
            "--password",
            "secret",
            "--json",
        ])
        .unwrap();

        match cli.command {
            Some(CliCommand::Transcribe(args)) => {
                assert_eq!(args.file, PathBuf::from("sample.wav"));
                assert_eq!(args.server.as_deref(), Some("localhost:47842"));
                assert_eq!(args.password.as_deref(), Some("secret"));
                assert!(args.json);
            }
            other => panic!("unexpected command: {:?}", other),
        }
    }

    #[test]
    fn cli_parses_record_command() {
        let cli =
            Cli::try_parse_from(["voicetypr", "record", "--until-silence", "--json"]).unwrap();

        match cli.command {
            Some(CliCommand::Record(args)) => {
                assert!(args.until_silence);
                assert!(args.json);
            }
            other => panic!("unexpected command: {:?}", other),
        }
    }
}
