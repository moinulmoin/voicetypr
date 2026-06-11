// TODO(plan-016 step 4): remove these allows when commands/ai.rs consumes the
// contract executor; they only bridge the phase gap until the cutover lands.
#[allow(dead_code)]
pub mod contract;
#[allow(dead_code)]
pub mod error;
#[allow(dead_code)]
pub mod executor;
#[allow(dead_code)]
pub mod genai_runtime;
pub mod openai;
#[allow(dead_code)]
pub mod openai_compatible;
pub mod prompts;
#[allow(dead_code)]
pub mod providers;

pub use prompts::EnhancementOptions;

#[cfg(test)]
mod runtime_tests;
#[cfg(test)]
mod tests;
