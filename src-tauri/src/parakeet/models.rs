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

pub static AVAILABLE_MODELS: Lazy<Vec<ParakeetModelDefinition>> = Lazy::new(|| {
    vec![
        ParakeetModelDefinition {
            id: "parakeet-tdt-0.6b-v3",
            display_name: "Parakeet V3",
            repo_id: "mlx-community/parakeet-tdt-0.6b-v3",
            description: "Parakeet TDT model converted for MLX",
            languages: &[
                "en", "es", "fr", "de", "bg", "hr", "cs", "da", "nl", "et", "fi", "el",
                "hu", "it", "lv", "lt", "mt", "pl", "pt", "ro", "sk", "sl", "sv", "ru", "uk",
            ],
            recommended: true,
            speed_score: 8,
            accuracy_score: 9,
            files: &[
                ParakeetModelFile { filename: "config.json" },
                ParakeetModelFile { filename: "model.safetensors" },
                ParakeetModelFile { filename: "tokenizer.model" },
                ParakeetModelFile { filename: "tokenizer.vocab" },
                ParakeetModelFile { filename: "vocab.txt" },
            ],
            estimated_size: 2_800_000_000,
        },
        ParakeetModelDefinition {
            id: "parakeet-tdt-0.6b-v2",
            display_name: "Parakeet V2",
            repo_id: "mlx-community/parakeet-tdt-0.6b-v2",
            description: "Parakeet TDT model converted for MLX (English)",
            languages: &["en"],
            recommended: false,
            speed_score: 9,
            accuracy_score: 8,
            files: &[
                ParakeetModelFile { filename: "config.json" },
                ParakeetModelFile { filename: "model.safetensors" },
                ParakeetModelFile { filename: "tokenizer.model" },
                ParakeetModelFile { filename: "tokenizer.vocab" },
                ParakeetModelFile { filename: "vocab.txt" },
            ],
            estimated_size: 2_500_000_000,
        },
    ]
});
