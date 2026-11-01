// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, useState } from "react";
import { useApi, type LibraryInfo } from "./api";

export default function App() {
  const api = useApi();
  const [info, setInfo] = useState<LibraryInfo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api.libraryInfo().then(setInfo, (e) => setError(String(e)));
  }, [api]);

  return (
    <main className="shell">
      <h1>Instagram Chat Viewer</h1>
      {error && <p className="error">{error}</p>}
      {!info && !error && <p className="muted">Opening library...</p>}
      {info && (
        <>
          <dl className="stats">
            <div>
              <dt>Chats</dt>
              <dd>{info.stats.threads.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Messages</dt>
              <dd>{info.stats.messages.toLocaleString()}</dd>
            </div>
            <div>
              <dt>Media files</dt>
              <dd>{info.stats.media.toLocaleString()}</dd>
            </div>
          </dl>
          <p className="muted">Library: {info.root}</p>
        </>
      )}
    </main>
  );
}
