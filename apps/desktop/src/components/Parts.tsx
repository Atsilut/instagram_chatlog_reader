// SPDX-License-Identifier: AGPL-3.0-or-later

import { useState } from "react";
import { useApi, type MediaView, type PartView } from "../api";
import { hostOf } from "../lib/format";

const MAX_W = 280;
const MAX_H = 360;

function boxSize(m: MediaView): { width: number; height: number } {
  const w = m.width ?? 240;
  const h = m.height ?? 240;
  const scale = Math.min(1, MAX_W / w, MAX_H / h);
  return { width: Math.round(w * scale), height: Math.round(h * scale) };
}

function Missing({ label }: { label: string }) {
  return (
    <div className="flex h-24 w-56 flex-col items-center justify-center gap-1 rounded-xl border border-dashed border-zinc-300 text-xs text-zinc-500 dark:border-zinc-700">
      <span>{label} not in your backups</span>
      <span className="text-[11px] opacity-70">Import a backup that includes it</span>
    </div>
  );
}

export function Part({ part, onOpenImage }: { part: PartView; onOpenImage: (src: string) => void }) {
  const api = useApi();
  const media = part.media;
  const present = media?.present && media.storePath;

  switch (part.type) {
    case "photo":
    case "gif": {
      if (!present) return <Missing label={part.type === "gif" ? "GIF" : "Photo"} />;
      const src = api.mediaUrl(media.storePath!);
      return (
        <button className="block overflow-hidden rounded-xl" style={boxSize(media)} onClick={() => onOpenImage(src)}>
          <img src={src} loading="lazy" decoding="async" className="h-full w-full object-cover" alt="" />
        </button>
      );
    }
    case "video": {
      if (!present) return <Missing label="Video" />;
      return <video src={api.mediaUrl(media.storePath!)} controls preload="metadata" className="max-h-[360px] max-w-[280px] rounded-xl bg-black" />;
    }
    case "audio": {
      if (!present) return <Missing label="Voice message" />;
      return <audio src={api.mediaUrl(media.storePath!)} controls preload="none" className="h-10 w-64" />;
    }
    case "file": {
      if (!present) return <Missing label="File" />;
      const name = part.url?.split("/").pop() ?? "File";
      return <FileChip name={name} storePath={media.storePath!} />;
    }
    case "share":
      return <ShareCard part={part} />;
    case "link":
    case "remote_gif":
      return <LinkCard url={part.url} title={part.type === "remote_gif" ? "GIF from GIPHY" : part.caption} />;
    default:
      return null;
  }
}

function ShareCard({ part }: { part: PartView }) {
  const api = useApi();
  const [expanded, setExpanded] = useState(false);
  const kind = part.url?.includes("/reel/") ? "Reel" : part.url?.includes("/stories/") ? "Story" : part.url ? "Post" : "Profile";
  return (
    <div className="w-72 overflow-hidden rounded-xl border border-zinc-200 bg-white text-zinc-900 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-100">
      <div className="flex items-center gap-2 border-b border-zinc-100 px-3 py-2 dark:border-zinc-800">
        <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[11px] font-medium text-zinc-600 dark:bg-zinc-800 dark:text-zinc-300">{kind}</span>
        {part.author && <span className="truncate text-sm font-semibold">@{part.author}</span>}
      </div>
      {part.caption && (
        <button
          className={`block w-full px-3 py-2 text-left text-sm whitespace-pre-wrap ${expanded ? "" : "line-clamp-4"}`}
          onClick={() => setExpanded(!expanded)}
        >
          {part.caption}
        </button>
      )}
      {part.url && (
        <button className="w-full px-3 py-2 text-left text-xs font-medium text-indigo-600 hover:underline dark:text-indigo-400" onClick={() => api.openUrl(part.url!)}>
          {isInstagram(part.url) ? "Open on Instagram" : `Open ${hostOf(part.url)}`}
        </button>
      )}
    </div>
  );
}

function FileChip({ name, storePath }: { name: string; storePath: string }) {
  const api = useApi();
  const [failed, setFailed] = useState(false);
  return (
    <button
      className="flex items-center gap-2 rounded-xl border border-zinc-200 px-3 py-2 text-left text-sm hover:bg-zinc-50 dark:border-zinc-700 dark:hover:bg-zinc-800"
      title="Show in folder"
      onClick={() => {
        setFailed(false);
        api.revealMedia(storePath).catch(() => setFailed(true));
      }}
    >
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z" />
        <path d="M14 3v6h6" />
      </svg>
      <span className="truncate">{failed ? "Couldn't show this file" : name}</span>
    </button>
  );
}

function isInstagram(url: string): boolean {
  const host = hostOf(url);
  return host === "instagram.com" || host.endsWith(".instagram.com");
}

function LinkCard({ url, title }: { url: string | null; title: string | null }) {
  const api = useApi();
  if (!url) return null;
  return (
    <button
      className="flex w-72 flex-col items-start gap-0.5 rounded-xl border border-zinc-200 bg-white px-3 py-2 text-left text-zinc-900 hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-100 dark:hover:bg-zinc-800"
      onClick={() => api.openUrl(url)}
    >
      {title && <span className="line-clamp-2 text-sm font-medium">{title}</span>}
      <span className="text-xs text-zinc-500">{hostOf(url)}</span>
    </button>
  );
}

export function Lightbox({ src, onClose }: { src: string; onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/90 p-8" onClick={onClose}>
      <img src={src} className="max-h-full max-w-full object-contain" alt="" />
    </div>
  );
}
