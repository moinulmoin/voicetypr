import React from "react";
import ReactDOM from "react-dom/client";
import { RecordingPill } from "./components/RecordingPill";
import { SettingsProvider } from "./contexts/SettingsContext";
import "./globals.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <SettingsProvider>
      <div className="w-screen h-screen overflow-hidden bg-transparent">
        <RecordingPill />
      </div>
    </SettingsProvider>
  </React.StrictMode>,
);