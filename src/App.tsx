import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Mic, MicOff, Settings, Download, Check, Loader2 } from 'lucide-react';
import { useRecording } from './hooks/useRecording';

// Types
interface ModelInfo {
  name: string;
  size: number;
  url: string;
  downloaded: boolean;
}

interface AppSettings {
  hotkey: string;
  current_model: string;
  language: string;
  auto_insert: boolean;
  show_window_on_record: boolean;
  theme: string;
}

interface TranscriptionHistory {
  id: string;
  text: string;
  timestamp: Date;
  model: string;
}

// Main App Component
export default function App() {
  const recording = useRecording();
  const [currentView, setCurrentView] = useState<'recorder' | 'settings' | 'onboarding'>('recorder');
  const [models, setModels] = useState<Record<string, ModelInfo>>({});
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [history, setHistory] = useState<TranscriptionHistory[]>([]);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});

  // Initialize app
  useEffect(() => {
    const init = async () => {
      try {
        // Load settings
        const appSettings = await invoke<AppSettings>('get_settings');
        setSettings(appSettings);

        // Load model status
        const modelStatus = await invoke<Record<string, ModelInfo>>('get_model_status');
        console.log('Model status from backend:', modelStatus);
        setModels(modelStatus);

        // Check if any model is downloaded
        const hasModel = Object.values(modelStatus).some(m => m.downloaded);
        console.log('Has downloaded model:', hasModel);
        if (!hasModel) {
          setCurrentView('onboarding');
        }

        // All recording event handling is now managed by the useRecording hook
        
        // Listen for no-models event to redirect to onboarding
        const handleNoModels = () => {
          console.log('No models available - redirecting to onboarding');
          setCurrentView('onboarding');
        };
        window.addEventListener('no-models-available', handleNoModels);

        const unlistenTranscription = await listen<string>('transcription-complete', async (event) => {
          console.log('Transcription complete:', event.payload);
          
          const newEntry: TranscriptionHistory = {
            id: Date.now().toString(),
            text: event.payload,
            timestamp: new Date(),
            model: settings?.current_model || 'base'
          };
          setHistory(prev => [newEntry, ...prev].slice(0, 50)); // Keep last 50

          // Copy to clipboard or insert at cursor
          if (settings?.auto_insert) {
            try {
              // Use the native text insertion command
              await invoke('insert_text', { text: event.payload });
              console.log('Text inserted via native keyboard simulation');
            } catch (e) {
              // Fallback to clipboard
              console.error('Failed to insert text, using clipboard:', e);
              navigator.clipboard.writeText(event.payload);
            }
          } else {
            // Just copy to clipboard
            navigator.clipboard.writeText(event.payload);
          }
        });

        const unlistenProgress = await listen<{model: string, progress: number}>('download-progress', (event) => {
          setDownloadProgress(prev => ({
            ...prev,
            [event.payload.model]: event.payload.progress
          }));
        });

        const unlistenDownloaded = await listen<string>('model-downloaded', (event) => {
          setModels(prev => ({
            ...prev,
            [event.payload]: { ...prev[event.payload], downloaded: true }
          }));
          setDownloadProgress(prev => {
            const newProgress = { ...prev };
            delete newProgress[event.payload];
            return newProgress;
          });
        });

        return () => {
          unlistenTranscription();
          unlistenProgress();
          unlistenDownloaded();
          window.removeEventListener('no-models-available', handleNoModels);
        };
      } catch (error) {
        console.error('Failed to initialize:', error);
      }
    };

    init();
  }, []);

  // Download model
  const downloadModel = async (modelName: string) => {
    try {
      console.log(`Starting download for model: ${modelName}`);
      // Set initial progress to show download started
      setDownloadProgress(prev => ({
        ...prev,
        [modelName]: 0
      }));
      
      // Don't await - let it run async so progress events can update UI
      invoke('download_model', { modelName }).catch((error) => {
        console.error('Failed to download model:', error);
        alert(`Failed to download model: ${error}`);
        // Remove from progress on error
        setDownloadProgress(prev => {
          const newProgress = { ...prev };
          delete newProgress[modelName];
          return newProgress;
        });
      });
    } catch (error) {
      console.error('Failed to start download:', error);
      alert(`Failed to start download: ${error}`);
    }
  };

  // Save settings
  const saveSettings = async (newSettings: AppSettings) => {
    try {
      await invoke('save_settings', { settings: newSettings });

      // Update global shortcut in backend if changed
      if (newSettings.hotkey !== settings?.hotkey) {
        await invoke('set_global_shortcut', { shortcut: newSettings.hotkey });
      }

      setSettings(newSettings);
    } catch (error) {
      console.error('Failed to save settings:', error);
    }
  };

  // Onboarding View
  if (currentView === 'onboarding') {
    return (
      <div className="flex flex-col h-screen bg-gray-50 dark:bg-gray-900">
        <div className="flex-1 flex flex-col items-center justify-center p-8">
          <h1 className="text-4xl font-bold mb-2 text-gray-900 dark:text-white">
            Welcome to VoiceType
          </h1>
          <p className="text-lg text-gray-600 dark:text-gray-400 mb-8">
            Choose a model to get started
          </p>

          <div className="space-y-4 w-full max-w-md">
            {Object.entries(models).map(([name, model]) => (
              <div
                key={name}
                className="border rounded-lg p-4 hover:border-blue-500 transition-colors bg-white dark:bg-gray-800"
              >
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className="font-semibold text-gray-900 dark:text-white capitalize">
                      {name} Model
                    </h3>
                    <p className="text-sm text-gray-500 dark:text-gray-400">
                      {(model.size / 1024 / 1024).toFixed(0)} MB
                      {name.includes('.en') && ' • English only'}
                      {name === 'tiny' && ' • Fastest, least accurate'}
                      {name === 'base' && ' • Good balance'}
                      {name === 'small' && ' • Best accuracy'}
                    </p>
                  </div>

                  {model.downloaded ? (
                    <button
                      onClick={async () => {
                        // Create default settings if none exist
                        const newSettings = settings || {
                          hotkey: 'CommandOrControl+Shift+Space',
                          language: 'auto',
                          auto_insert: true,
                          show_window_on_record: false,
                          theme: 'system'
                        };
                        
                        // Save with selected model
                        await saveSettings({ ...newSettings, current_model: name });
                        setCurrentView('recorder');
                      }}
                      className="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 flex items-center gap-2"
                    >
                      <Check className="w-4 h-4" />
                      Select
                    </button>
                  ) : downloadProgress[name] !== undefined ? (
                    <div className="flex items-center gap-2">
                      <Loader2 className="w-4 h-4 animate-spin" />
                      <span className="text-sm">{downloadProgress[name].toFixed(0)}%</span>
                    </div>
                  ) : (
                    <button
                      onClick={() => downloadModel(name)}
                      className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 flex items-center gap-2"
                    >
                      <Download className="w-4 h-4" />
                      Download
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  // Settings View
  if (currentView === 'settings') {
    return (
      <div className="flex flex-col h-screen bg-gray-50 dark:bg-gray-900">
        <div className="flex items-center justify-between p-4 border-b dark:border-gray-700">
          <h2 className="text-lg font-semibold text-gray-900 dark:text-white">Settings</h2>
          <button
            onClick={() => setCurrentView('recorder')}
            className="text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
          >
            ✕
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-4 space-y-6">
          {/* Hotkey Setting */}
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              Recording Hotkey
            </label>
            <input
              type="text"
              value={settings?.hotkey || ''}
              onChange={(e) => saveSettings({ ...settings!, hotkey: e.target.value })}
              className="w-full px-3 py-2 border rounded-lg dark:bg-gray-800 dark:border-gray-600"
              placeholder="CommandOrControl+Shift+Space"
            />
          </div>

          {/* Model Selection */}
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              Whisper Model
            </label>
            <select
              value={settings?.current_model || 'base'}
              onChange={(e) => saveSettings({ ...settings!, current_model: e.target.value })}
              className="w-full px-3 py-2 border rounded-lg dark:bg-gray-800 dark:border-gray-600"
            >
              {Object.entries(models).filter(([_, m]) => m.downloaded).map(([name]) => (
                <option key={name} value={name}>{name}</option>
              ))}
            </select>
          </div>

          {/* Language Setting */}
          <div>
            <label className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              Language
            </label>
            <select
              value={settings?.language || 'auto'}
              onChange={(e) => saveSettings({ ...settings!, language: e.target.value })}
              className="w-full px-3 py-2 border rounded-lg dark:bg-gray-800 dark:border-gray-600"
            >
              <option value="auto">Auto-detect</option>
              <option value="en">English</option>
              <option value="es">Spanish</option>
              <option value="fr">French</option>
              <option value="de">German</option>
              <option value="it">Italian</option>
              <option value="pt">Portuguese</option>
              <option value="ru">Russian</option>
              <option value="ja">Japanese</option>
              <option value="ko">Korean</option>
              <option value="zh">Chinese</option>
            </select>
          </div>

          {/* Auto Insert Toggle */}
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
              Auto-insert text at cursor
            </label>
            <input
              type="checkbox"
              checked={settings?.auto_insert || false}
              onChange={(e) => saveSettings({ ...settings!, auto_insert: e.target.checked })}
              className="rounded"
            />
          </div>

          {/* Show Window on Record Toggle */}
          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
              Show window when recording
            </label>
            <input
              type="checkbox"
              checked={settings?.show_window_on_record || false}
              onChange={(e) => saveSettings({ ...settings!, show_window_on_record: e.target.checked })}
              className="rounded"
            />
          </div>

          {/* Model Management */}
          <div>
            <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              Manage Models
            </h3>
            <div className="space-y-2">
              {Object.entries(models).map(([name, model]) => (
                <div key={name} className="flex items-center justify-between p-2 border rounded dark:border-gray-600">
                  <div>
                    <span className="capitalize">{name}</span>
                    <span className="text-sm text-gray-500 ml-2">
                      ({(model.size / 1024 / 1024).toFixed(0)} MB)
                    </span>
                  </div>
                  {model.downloaded ? (
                    <span className="text-green-500 text-sm">Downloaded</span>
                  ) : downloadProgress[name] !== undefined ? (
                    <div className="flex items-center gap-2">
                      <Loader2 className="w-4 h-4 animate-spin" />
                      <span className="text-sm">{downloadProgress[name].toFixed(0)}%</span>
                    </div>
                  ) : (
                    <button
                      onClick={() => downloadModel(name)}
                      className="text-blue-500 text-sm hover:underline"
                    >
                      Download
                    </button>
                  )}
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Main Recorder View
  return (
    <div className="flex flex-col h-screen bg-gray-50 dark:bg-gray-900">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b dark:border-gray-700">
        <h1 className="text-lg font-semibold text-gray-900 dark:text-white">VoiceType</h1>
        <button
          onClick={() => setCurrentView('settings')}
          className="p-2 hover:bg-gray-200 dark:hover:bg-gray-700 rounded-lg"
        >
          <Settings className="w-5 h-5" />
        </button>
      </div>

      {/* Recording Status */}
      <div className="flex-1 flex flex-col items-center justify-center p-8">
        <div className={`relative rounded-full p-8 transition-all ${
          recording.state === 'recording' || recording.state === 'starting'
            ? 'bg-red-100 dark:bg-red-900/30 animate-pulse scale-110'
            : recording.state === 'transcribing' 
            ? 'bg-blue-100 dark:bg-blue-900/30'
            : 'bg-gray-200 dark:bg-gray-700'
        }`}>
          {recording.state === 'recording' || recording.state === 'starting' ? (
            <>
              <Mic className="w-16 h-16 text-red-500" />
              <div className="absolute inset-0 rounded-full border-4 border-red-500 animate-ping" />
            </>
          ) : recording.state === 'transcribing' ? (
            <Loader2 className="w-16 h-16 text-blue-500 animate-spin" />
          ) : (
            <MicOff className="w-16 h-16 text-gray-400" />
          )}
        </div>

        <p className="mt-6 text-lg text-gray-600 dark:text-gray-400">
          {recording.state === 'idle' && `Press ${settings?.hotkey || 'hotkey'} to record`}
          {recording.state === 'starting' && 'Starting recording...'}
          {recording.state === 'recording' && 'Recording...'}
          {recording.state === 'stopping' && 'Stopping...'}
          {recording.state === 'transcribing' && 'Transcribing your speech...'}
          {recording.state === 'error' && 'Error occurred'}
        </p>
        
        {/* Show detailed state for debugging */}
        {recording.error && (
          <p className="mt-2 text-sm text-red-500">Error: {recording.error}</p>
        )}
        
        {/* Manual test button - Toggle recording */}
        <button
          className={`mt-4 px-6 py-3 rounded-lg font-medium transition-all ${
            recording.state === 'recording' || recording.state === 'starting'
              ? 'bg-red-500 hover:bg-red-600 text-white' 
              : recording.state === 'transcribing' || recording.state === 'stopping'
              ? 'bg-gray-400 text-white cursor-not-allowed'
              : 'bg-blue-500 hover:bg-blue-600 text-white'
          }`}
          onClick={async () => {
            if (recording.state === 'recording') {
              await recording.stopRecording();
            } else if (recording.state === 'idle' || recording.state === 'error') {
              await recording.startRecording();
            }
            // Do nothing if transcribing, stopping, or starting
          }}
          disabled={recording.state === 'transcribing' || recording.state === 'stopping' || recording.state === 'starting'}
        >
          {recording.state === 'idle' && 'Start Recording'}
          {recording.state === 'starting' && 'Starting...'}
          {recording.state === 'recording' && 'Stop Recording'}
          {recording.state === 'stopping' && 'Stopping...'}
          {recording.state === 'transcribing' && 'Transcribing...'}
          {recording.state === 'error' && 'Try Again'}
        </button>
        
        {recording.state === 'recording' && (
          <div className="mt-4 flex items-center gap-2 text-sm text-red-500">
            <div className="w-2 h-2 bg-red-500 rounded-full animate-pulse" />
            <span>Recording in progress</span>
          </div>
        )}
      </div>

      {/* History */}
      {history.length > 0 && (
        <div className="border-t dark:border-gray-700 p-4">
          <h3 className="text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
            Recent Transcriptions
          </h3>
          <div className="space-y-2 max-h-48 overflow-y-auto">
            {history.slice(0, 5).map((item) => (
              <div
                key={item.id}
                className="p-2 bg-white dark:bg-gray-800 rounded border dark:border-gray-700 cursor-pointer hover:bg-gray-50 dark:hover:bg-gray-700"
                onClick={() => navigator.clipboard.writeText(item.text)}
              >
                <p className="text-sm text-gray-900 dark:text-white truncate">
                  {item.text}
                </p>
                <p className="text-xs text-gray-500 dark:text-gray-400">
                  {new Date(item.timestamp).toLocaleTimeString()} • {item.model}
                </p>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
