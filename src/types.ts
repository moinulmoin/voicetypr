export type SpeechModelEngine = 'whisper' | 'parakeet' | 'soniox';
export type ModelKind = 'local' | 'cloud';

interface BaseModelInfo {
  name: string;
  display_name: string;
  engine: SpeechModelEngine;
  kind: ModelKind;
  recommended: boolean;
  downloaded: boolean;
  requires_setup: boolean;
  speed_score?: number; // Optional for cloud providers
  accuracy_score?: number; // Optional for cloud providers
  size?: number;
  url?: string;
  sha256?: string;
}

export interface LocalModelInfo extends BaseModelInfo {
  kind: 'local';
  size: number;
  url: string;
  sha256: string;
  speed_score: number;
  accuracy_score: number;
}

export interface CloudModelInfo extends BaseModelInfo {
  kind: 'cloud';
}

export type ModelInfo = LocalModelInfo | CloudModelInfo;

export const isCloudModel = (model: ModelInfo): model is CloudModelInfo =>
  model.kind === 'cloud';

export const isLocalModel = (model: ModelInfo): model is LocalModelInfo =>
  model.kind === 'local';

export type RecordingMode = 'toggle' | 'push_to_talk';

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
  check_updates_automatically?: boolean;
  selected_microphone?: string | null;
  // Push-to-talk support
  recording_mode?: RecordingMode;
  use_different_ptt_key?: boolean;
  ptt_hotkey?: string;
  current_model_engine?: 'whisper' | 'parakeet' | 'soniox';
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
