use std::collections::HashMap;
use std::time::Instant;

/// Comprehensive onboarding flow logger
/// Captures EVERY step to debug any onboarding issues
pub struct OnboardingLogger {
    session_id: String,
    start_time: Instant,
    steps: Vec<OnboardingStep>,
}

#[derive(Debug, Clone)]
struct OnboardingStep {
    timestamp: Instant,
    step_name: String,
    status: StepStatus,
    details: HashMap<String, String>,
}

#[derive(Debug, Clone)]
enum StepStatus {
    Started,
    InProgress(u8), // Percentage for downloads
    Completed,
    Failed(String), // Error message
    Skipped,
}

impl OnboardingLogger {
    pub fn start_session() -> Self {
        let session_id = format!("onboard_{}", chrono::Utc::now().timestamp());
        log::info!("🚀 ONBOARDING_START - Session: {}", session_id);
        
        Self {
            session_id,
            start_time: Instant::now(),
            steps: Vec::new(),
        }
    }
    
    /// Log permission request
    pub fn log_permission_request(&mut self, permission_type: &str) {
        log::info!("🔒 PERMISSION_REQUEST - Type: {}", permission_type);
        self.add_step("permission_request", StepStatus::Started, vec![
            ("type", permission_type),
        ]);
    }
    
    /// Log permission result
    pub fn log_permission_result(&mut self, permission_type: &str, granted: bool) {
        if granted {
            log::info!("✅ PERMISSION_GRANTED - Type: {}", permission_type);
            self.add_step("permission_granted", StepStatus::Completed, vec![
                ("type", permission_type),
            ]);
        } else {
            log::error!("❌ PERMISSION_DENIED - Type: {}", permission_type);
            self.add_step("permission_denied", 
                StepStatus::Failed("User denied permission".to_string()), 
                vec![("type", permission_type)]
            );
        }
    }
    
    /// Log model download start
    pub fn log_model_download_start(&mut self, model_name: &str, size_mb: u64) {
        log::info!("📥 MODEL_DOWNLOAD_START - Model: {}, Size: {}MB", model_name, size_mb);
        self.add_step("model_download", StepStatus::Started, vec![
            ("model", model_name),
            ("size_mb", &size_mb.to_string()),
        ]);
    }
    
    /// Log model download progress
    pub fn log_model_download_progress(&mut self, model_name: &str, progress: u8) {
        // Log every 10% or at critical points
        if progress % 10 == 0 || progress == 1 || progress == 99 {
            log::info!("📊 MODEL_DOWNLOAD_PROGRESS - Model: {}, Progress: {}%", model_name, progress);
            self.add_step("model_download", StepStatus::InProgress(progress), vec![
                ("model", model_name),
                ("progress", &progress.to_string()),
            ]);
        }
    }
    
    /// Log model download completion
    pub fn log_model_download_complete(&mut self, model_name: &str, duration_ms: u64) {
        log::info!("✅ MODEL_DOWNLOAD_COMPLETE - Model: {}, Duration: {}ms", model_name, duration_ms);
        self.add_step("model_download", StepStatus::Completed, vec![
            ("model", model_name),
            ("duration_ms", &duration_ms.to_string()),
        ]);
    }
    
    /// Log model download failure
    pub fn log_model_download_failed(&mut self, model_name: &str, error: &str) {
        log::error!("❌ MODEL_DOWNLOAD_FAILED - Model: {}, Error: {}", model_name, error);
        self.add_step("model_download", 
            StepStatus::Failed(error.to_string()),
            vec![("model", model_name)]
        );
    }
    
    /// Log hardware detection
    pub fn log_hardware_detection(&mut self, gpu_available: bool, cpu_cores: usize) {
        let hw_type = if gpu_available { "GPU" } else { "CPU" };
        log::info!("🎮 HARDWARE_DETECTION - Type: {}, Cores: {}", hw_type, cpu_cores);
        self.add_step("hardware_detection", StepStatus::Completed, vec![
            ("type", hw_type),
            ("cpu_cores", &cpu_cores.to_string()),
            ("gpu_available", &gpu_available.to_string()),
        ]);
    }
    
    /// Log test recording
    pub fn log_test_recording(&mut self, success: bool, transcription: Option<&str>) {
        if success {
            log::info!("✅ TEST_RECORDING_SUCCESS - Transcription: {:?}", transcription);
            self.add_step("test_recording", StepStatus::Completed, vec![
                ("success", "true"),
                ("transcription", transcription.unwrap_or("none")),
            ]);
        } else {
            log::error!("❌ TEST_RECORDING_FAILED");
            self.add_step("test_recording", 
                StepStatus::Failed("Test recording failed".to_string()),
                vec![("success", "false")]
            );
        }
    }
    
    /// Log hotkey registration
    pub fn log_hotkey_registration(&mut self, hotkey: &str, success: bool, conflict: Option<&str>) {
        if success {
            log::info!("✅ HOTKEY_REGISTERED - Hotkey: {}", hotkey);
            self.add_step("hotkey_registration", StepStatus::Completed, vec![
                ("hotkey", hotkey),
            ]);
        } else {
            let error_msg = conflict.unwrap_or("Registration failed");
            log::error!("❌ HOTKEY_FAILED - Hotkey: {}, Conflict: {}", hotkey, error_msg);
            self.add_step("hotkey_registration",
                StepStatus::Failed(error_msg.to_string()),
                vec![("hotkey", hotkey)]
            );
        }
    }
    
    /// Complete onboarding session
    pub fn complete_onboarding(&self, success: bool) {
        let duration = self.start_time.elapsed();
        let step_count = self.steps.len();
        
        if success {
            log::info!("🎉 ONBOARDING_COMPLETE - Steps: {}, Duration: {}ms", 
                      step_count, duration.as_millis());
        } else {
            log::error!("❌ ONBOARDING_FAILED - Steps: {}, Duration: {}ms",
                       step_count, duration.as_millis());
            
            // Log complete failure analysis
            log::error!("📋 ONBOARDING_FAILURE_ANALYSIS:");
            for step in &self.steps {
                if let StepStatus::Failed(error) = &step.status {
                    let elapsed = step.timestamp.duration_since(self.start_time).as_millis();
                    log::error!("  ❌ {} at +{}ms: {}", step.step_name, elapsed, error);
                }
            }
        }
        
        // Always log complete session for debugging
        self.log_session_summary();
    }
    
    /// Internal: Add step to tracking
    fn add_step(&mut self, name: &str, status: StepStatus, details: Vec<(&str, &str)>) {
        let mut details_map = HashMap::new();
        for (k, v) in details {
            details_map.insert(k.to_string(), v.to_string());
        }
        
        self.steps.push(OnboardingStep {
            timestamp: Instant::now(),
            step_name: name.to_string(),
            status,
            details: details_map,
        });
    }
    
    /// Log complete session summary
    fn log_session_summary(&self) {
        log::info!("📊 ONBOARDING_SESSION_SUMMARY - {}", self.session_id);
        log::info!("  Total steps: {}", self.steps.len());
        log::info!("  Duration: {}ms", self.start_time.elapsed().as_millis());
        
        for step in &self.steps {
            let elapsed = step.timestamp.duration_since(self.start_time).as_millis();
            let status_str = match &step.status {
                StepStatus::Started => "STARTED",
                StepStatus::InProgress(p) => &format!("IN_PROGRESS({}%)", p),
                StepStatus::Completed => "COMPLETED ✅",
                StepStatus::Failed(_) => "FAILED ❌",
                StepStatus::Skipped => "SKIPPED",
            };
            
            log::info!("  [{:>6}ms] {} - {}", elapsed, step.step_name, status_str);
        }
    }
}

use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;

/// Thread-safe global onboarding logger instance
static ONBOARDING_LOGGER: Lazy<Arc<Mutex<Option<OnboardingLogger>>>> = 
    Lazy::new(|| Arc::new(Mutex::new(None)));

/// Start onboarding session (thread-safe)
pub fn start_onboarding() {
    let mut logger = ONBOARDING_LOGGER.lock().unwrap();
    *logger = Some(OnboardingLogger::start_session());
}

/// Get current onboarding logger (thread-safe)
pub fn with_onboarding_logger<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut OnboardingLogger) -> R,
{
    let mut logger = ONBOARDING_LOGGER.lock().unwrap();
    logger.as_mut().map(f)
}