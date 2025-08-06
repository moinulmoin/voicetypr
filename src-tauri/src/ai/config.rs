/// Configuration constants for AI providers

/// Default timeout for API requests in seconds
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum number of retry attempts for API calls
pub const MAX_RETRIES: u32 = 3;

/// Base delay in milliseconds for exponential backoff
pub const RETRY_BASE_DELAY_MS: u64 = 1000;

/// Minimum API key length for basic validation
pub const MIN_API_KEY_LENGTH: usize = 10;

/// Default temperature for AI models (0.0 = deterministic, 1.0 = creative)
pub const DEFAULT_TEMPERATURE: f32 = 0.5;

/// Maximum text length for enhancement requests
pub const MAX_TEXT_LENGTH: usize = 10_000;

/// Maximum custom vocabulary terms allowed
pub const MAX_CUSTOM_VOCABULARY: usize = 50;

/// Maximum length for each vocabulary term
pub const MAX_VOCABULARY_TERM_LENGTH: usize = 100;
