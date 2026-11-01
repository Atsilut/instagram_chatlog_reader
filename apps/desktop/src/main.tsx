// SPDX-License-Identifier: AGPL-3.0-or-later

import React from "react";
import ReactDOM from "react-dom/client";
import { ApiContext, createApiClient } from "./api";
import App from "./App";
import "./App.css";

const api = createApiClient();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {api ? (
      <ApiContext.Provider value={api}>
        <App />
      </ApiContext.Provider>
    ) : (
      <main className="shell">
        <h1>Instagram Chat Viewer</h1>
        <p className="muted">
          No backend found. Open this through the desktop app (npm run tauri dev). Browser mode is
          planned for the Docker release.
        </p>
      </main>
    )}
  </React.StrictMode>,
);
