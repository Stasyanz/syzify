import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initTheme } from "./lib/theme";
import { initExternalLinks } from "./lib/externalLinks";

// Apply the persisted Trailhead theme before first paint to avoid a flash.
initTheme();
// Route external links (e.g. the Leaflet map attribution) to the system
// browser so they can't navigate the app webview away from itself.
initExternalLinks();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
