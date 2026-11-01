// SPDX-License-Identifier: AGPL-3.0-or-later

import React from "react";
import ReactDOM from "react-dom/client";
import { ApiContext, createApiClient } from "./api";
import App from "./App";
import "./index.css";

const api = createApiClient();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {api ? (
      <ApiContext.Provider value={api}>
        <App />
      </ApiContext.Provider>
    ) : (
      <main className="mx-auto mt-[15vh] max-w-xl px-6">
        <h1 className="mb-4 text-2xl font-bold">Instagram Chat Viewer</h1>
        <p className="text-sm text-zinc-500">
          No backend found. Open this through the desktop app (npm run tauri dev). Browser mode is planned for the Docker release.
        </p>
      </main>
    )}
  </React.StrictMode>,
);
