use once_cell::sync::Lazy;

#[derive(Debug, Clone)]
pub struct ParakeetModelFile {
    pub filename: &'static str,
}

#[derive(Debug, Clone)]
pub struct ParakeetModelDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub repo_id: &'static str,
    pub description: &'static str,
    pub languages: &'static [&'static str],
    pub recommended: bool,
    pub speed_score: u8,
    pub accuracy_score: u8,
    pub files: &'static [ParakeetModelFile],
    pub estimated_size: u64,
    /// If true, this model has additional restrictions beyond the base Apple Silicon requirement.
    /// Note: ALL Parakeet models require Apple Silicon (FluidAudio uses Apple Neural Engine).
    /// This flag indicates models that have extra compatibility issues (e.g., V2 SIGFPE crashes).
    pub apple_silicon_only: bool,
}

/// Check if running on Apple Silicon (aarch64)
pub fn is_apple_silicon() -> bool {
    std::env::consts::ARCH == "aarch64"
}

/// Get available models for the current architecture.
///
/// **Important**: FluidAudio ASR requires Apple Silicon (Apple Neural Engine).
/// On Intel Macs, this returns an empty vector because FluidAudio throws
/// `ASRError.unsupportedPlatform` at runtime for ALL Parakeet models.
///
/// Intel Mac users should use Whisper models instead (CPU-only mode).
pub fn get_available_models() -> Vec<&'static ParakeetModelDefinition> {
    let arch = std::env::consts::ARCH;

    // FluidAudio ASR requires Apple Silicon - no Parakeet models work on Intel Macs
    // FluidAudio throws ASRError.unsupportedPlatform("Parakeet models require Apple Silicon")
    if !is_apple_silicon() {
        log::info!(
            "🚫 Parakeet unavailable on Intel Mac - FluidAudio requires Apple Neural Engine (arch: {})",
            arch
        );
        return vec![];
    }

    // On Apple Silicon, return all available models
    AVAILABLE_MODELS.iter().collect()
}

// Parakeet models using Swift/FluidAudio sidecar
// These models are macOS-only and use Apple Neural Engine for acceleration
pub static AVAILABLE_MODELS: Lazy<Vec<ParakeetModelDefinition>> = Lazy::new(|| {
    vec![
        ParakeetModelDefinition {
            id: "parakeet-tdt-0.6b-v3",
            display_name: "Parakeet V3",
            repo_id: "FluidInference/parakeet-tdt-0.6b-v3-coreml",
            description: "Native Swift transcription using Apple Neural Engine",
            languages: &[
                "en", "es", "fr", "de", "bg", "hr", "cs", "da", "nl", "et", "fi", "el", "hu", "it",
                "lv", "lt", "mt", "pl", "pt", "ro", "sk", "sl", "sv", "ru", "uk",
            ],
            recommended: true,
            speed_score: 9,
            accuracy_score: 9,
            files: &[
                ParakeetModelFile {
                    filename: "Preprocessor.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "Encoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "Decoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "JointDecision.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "parakeet_vocab.json",
                },
            ],
            estimated_size: 500_000_000, // FluidAudio CoreML model is ~500MB
            apple_silicon_only: false,   // No additional restrictions beyond base Apple Silicon requirement
        },
        ParakeetModelDefinition {
            id: "parakeet-tdt-0.6b-v2",
            display_name: "Parakeet V2 (English)",
            repo_id: "FluidInference/parakeet-tdt-0.6b-v2-coreml",
            description: "Native Swift transcription optimized for English",
            languages: &["en"],
            recommended: true,
            speed_score: 10,
            accuracy_score: 8,
            files: &[
                ParakeetModelFile {
                    filename: "Preprocessor.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "Encoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "Decoder.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "JointDecision.mlmodelc",
                },
                ParakeetModelFile {
                    filename: "parakeet_vocab.json",
                },
            ],
            estimated_size: 480_000_000,
            apple_silicon_only: true, // V2 CoreML model crashes on Intel Macs (SIGFPE in Espresso)
        },
    ]
});
