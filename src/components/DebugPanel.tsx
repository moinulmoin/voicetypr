import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

export function DebugPanel() {
  const [debugInfo, setDebugInfo] = useState("");
  const [testText, setTestText] = useState("This is a test transcription");
  
  const runDebugFlow = async () => {
    try {
      const info = await invoke<string>("debug_transcription_flow");
      setDebugInfo(info);
    } catch (error) {
      setDebugInfo(`Error: ${error}`);
    }
  };
  
  const testTranscriptionEvent = async () => {
    try {
      await invoke("test_transcription_event", { text: testText });
      setDebugInfo(prev => prev + "\n\nTest transcription event sent!");
    } catch (error) {
      setDebugInfo(prev => prev + `\n\nError sending test event: ${error}`);
    }
  };
  
  if (process.env.NODE_ENV !== "development") {
    return null;
  }
  
  return (
    <div className="fixed bottom-4 left-4 bg-white border rounded-lg shadow-lg p-4 w-96 max-h-96 overflow-auto">
      <h3 className="font-bold mb-2">Debug Panel</h3>
      
      <div className="space-y-2">
        <Button onClick={runDebugFlow} size="sm" className="w-full">
          Run Debug Flow Check
        </Button>
        
        <div className="flex gap-2">
          <Textarea
            value={testText}
            onChange={(e) => setTestText(e.target.value)}
            placeholder="Test text"
            className="flex-1 h-20"
          />
          <Button onClick={testTranscriptionEvent} size="sm">
            Send Test Event
          </Button>
        </div>
        
        {debugInfo && (
          <pre className="text-xs bg-gray-100 p-2 rounded overflow-x-auto whitespace-pre-wrap">
            {debugInfo}
          </pre>
        )}
      </div>
    </div>
  );
}