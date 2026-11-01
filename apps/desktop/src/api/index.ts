// SPDX-License-Identifier: AGPL-3.0-or-later

import { createContext, useContext } from "react";
import type { ApiClient } from "./client";
import { TauriClient } from "./tauriClient";

export type { ApiClient } from "./client";
export * from "./types";

/** Returns null when no backend is reachable (e.g. the Vite page opened in a plain browser). */
export function createApiClient(): ApiClient | null {
  if ("__TAURI_INTERNALS__" in window) {
    return new TauriClient();
  }
  return null;
}

export const ApiContext = createContext<ApiClient | null>(null);

export function useApi(): ApiClient {
  const api = useContext(ApiContext);
  if (!api) throw new Error("useApi must be used inside ApiContext.Provider");
  return api;
}
