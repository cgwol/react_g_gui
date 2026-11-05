import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import "./settings";

try {
  if (getCurrentWindow().label === "overlay") {
    document.documentElement.classList.add("overlay");
  }
} catch {
  // Vite opened outside Tauri.
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>,
);
