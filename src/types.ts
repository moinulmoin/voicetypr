export interface ModelInfo {
  name: string;
  size: number;
  url: string;
  downloaded: boolean;
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