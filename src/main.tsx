import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./globals.css";
import { AppErrorBoundary } from "./components/ErrorBoundary";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary
      onError={(error, errorInfo) => {
        // Log to console in development
        console.error('Root error boundary caught:', error, errorInfo);
        // In production, this could send to an error tracking service
      }}
    >
      <App />
    </AppErrorBoundary>
  </React.StrictMode>,
);
