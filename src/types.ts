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
  auto_insert: boolean;
  show_window_on_record: boolean;
  theme: string;
}

export interface TranscriptionHistory {
  id: string;
  text: string;
  timestamp: Date;
  model: string;
}