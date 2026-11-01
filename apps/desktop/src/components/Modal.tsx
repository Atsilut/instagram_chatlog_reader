// SPDX-License-Identifier: AGPL-3.0-or-later

import { useEffect, type ReactNode } from "react";

interface Props {
  title: string;
  onClose?: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
}

export function Modal({ title, onClose, children, footer, wide }: Props) {
  useEffect(() => {
    if (!onClose) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/40 p-6 backdrop-blur-sm" onMouseDown={onClose}>
      <div
        className={`flex max-h-full w-full ${wide ? "max-w-2xl" : "max-w-md"} flex-col rounded-2xl bg-white shadow-2xl dark:bg-zinc-900`}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-b border-zinc-200 px-5 py-4 dark:border-zinc-800">
          <h2 className="text-base font-semibold">{title}</h2>
          {onClose && (
            <button className="rounded-full p-1 text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800" onClick={onClose} aria-label="Close">
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M6 6l12 12M18 6L6 18" />
              </svg>
            </button>
          )}
        </div>
        <div className="overflow-y-auto px-5 py-4">{children}</div>
        {footer && <div className="flex justify-end gap-2 border-t border-zinc-200 px-5 py-3 dark:border-zinc-800">{footer}</div>}
      </div>
    </div>
  );
}

export function Button({
  children,
  onClick,
  primary,
  disabled,
}: {
  children: ReactNode;
  onClick?: () => void;
  primary?: boolean;
  disabled?: boolean;
}) {
  const style = primary
    ? "bg-indigo-600 text-white hover:bg-indigo-500 disabled:bg-indigo-600/40"
    : "bg-zinc-100 text-zinc-800 hover:bg-zinc-200 dark:bg-zinc-800 dark:text-zinc-100 dark:hover:bg-zinc-700 disabled:opacity-40";
  return (
    <button className={`rounded-lg px-3.5 py-2 text-sm font-medium transition-colors ${style}`} onClick={onClick} disabled={disabled}>
      {children}
    </button>
  );
}
