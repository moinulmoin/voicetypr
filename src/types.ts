export interface ModelInfo {
  name: string;
  size: number;
  url: string;
  downloaded: boolean;
  speed_score: number;     // 1-10, 10 being fastest
  accuracy_score: number;  // 1-10, 10 being most accurate
}

export interface AppSettings {
  hotkey: string;
  current_model: string;
  language: string;
  translate_to_english?: boolean;
  theme: string;
  transcription_cleanup_days?: number | null;
  launch_at_startup?: boolean;
  onboarding_completed?: boolean;
  compact_recording_status?: boolean;
}

export interface TranscriptionHistory {
  id: string;
  text: string;
  timestamp: Date;
  model: string;
}

export interface LicenseStatus {
  status: 'licensed' | 'trial' | 'expired' | 'none';
  trial_days_left?: number;
  license_type?: string;
  license_key?: string;
  expires_at?: string;
}