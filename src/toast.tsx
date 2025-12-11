import React from "react";
import ReactDOM from "react-dom/client";
import { FeedbackToast } from "./components/FeedbackToast";
import "./globals.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <div className="w-screen h-screen overflow-hidden bg-transparent">
      <FeedbackToast />
    </div>
  </React.StrictMode>,
);
