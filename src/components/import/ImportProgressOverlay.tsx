import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

interface ImportProgress {
  current: number;
  total: number;
  filename: string;
}

export function ImportProgressOverlay() {
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    let hideTimer: ReturnType<typeof setTimeout> | null = null;

    const unlisten = listen<ImportProgress>("import:progress", (event) => {
      const p = event.payload;
      setProgress(p);
      setVisible(true);

      // Auto-hide 600ms after last file reaches total
      if (hideTimer) clearTimeout(hideTimer);
      if (p.current >= p.total) {
        hideTimer = setTimeout(() => {
          setVisible(false);
          setProgress(null);
        }, 600);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
      if (hideTimer) clearTimeout(hideTimer);
    };
  }, []);

  if (!visible || !progress) return null;

  const pct = (progress.current / progress.total) * 100;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="bg-card rounded-xl shadow-2xl w-full max-w-sm mx-4 p-6 space-y-3">
        <h3 className="text-sm font-semibold text-ink">
          Importing activities...
        </h3>
        <div className="w-full bg-card-2 rounded-full h-2.5 overflow-hidden">
          <div
            className="bg-accent h-full rounded-full transition-all duration-300"
            style={{ width: `${pct}%` }}
          />
        </div>
        <div className="flex items-center justify-between text-xs text-muted">
          <span className="truncate max-w-[200px]">{progress.filename}</span>
          <span className="shrink-0 ml-2">
            {progress.current} / {progress.total}
          </span>
        </div>
      </div>
    </div>
  );
}
