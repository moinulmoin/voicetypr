import { RecordingPill } from "@/components/RecordingPill";

export function AboutSection() {
  return (
    <div className="p-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4">About VoiceType</h2>
      <div className="space-y-4">
        <div>
          <p className="text-sm text-gray-600 dark:text-gray-400">Version</p>
          <p className="text-base font-medium">0.1.0</p>
        </div>
        <div>
          <p className="text-sm text-gray-600 dark:text-gray-400">Description</p>
          <p className="text-base">Offline voice transcription for macOS</p>
        </div>
        
        {/* Debug RecordingPill styling */}
        <div className="mt-8 pt-8 border-t border-gray-200 dark:border-gray-800">
          <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">Recording Pill Preview (for debugging):</p>
          <div className="relative h-32 bg-gray-100 dark:bg-gray-800 rounded-lg">
            <RecordingPill />
          </div>
        </div>
      </div>
    </div>
  );
}