import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./globals.css";
import { AppErrorBoundary } from "./components/ErrorBoundary";
import { createLogger } from "@/lib/logger";

const log = createLogger("app");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary
      onError={(error, errorInfo) => {
        log.error('Root error boundary caught:', error, errorInfo);
      }}
    >
      <App />
    </AppErrorBoundary>
  </React.StrictMode>,
);
