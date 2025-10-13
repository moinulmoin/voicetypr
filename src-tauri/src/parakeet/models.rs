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
        },
    ]
});
