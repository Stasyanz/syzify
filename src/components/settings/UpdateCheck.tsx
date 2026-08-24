import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { api } from "../../lib/tauri";

/** Manual "Check for updates" under the version in Settings → General.
 * Strictly user-initiated (the app never phones home on its own) with the
 * endpoint disclosed right on the row; an available update offers a one-click
 * signed install — the download still only starts on an explicit click. */
export function UpdateCheck() {
  const check = useMutation({ mutationFn: () => api.checkForUpdates() });
  const [progress, setProgress] = useState<number | null>(null);
  // Success never lands: the backend restarts the app after installing.
  const install = useMutation({
    mutationFn: async () => {
      const unlisten = await listen<{ downloaded: number; total: number | null }>(
        "update:progress",
        (e) => {
          const { downloaded, total } = e.payload;
          if (total) setProgress(Math.min(99, Math.floor((downloaded / total) * 100)));
        },
      );
      try {
        await api.installUpdate();
      } finally {
        unlisten();
        setProgress(null);
      }
    },
  });

  const upToDate = check.isSuccess && !check.data.update_available;
  // One line that changes in place — never a second appended line.
  const label = check.isPending
    ? "Checking…"
    : upToDate
      ? "You're up to date"
      : "Check for updates";

  return (
    <div className="flex flex-col items-end gap-0.5 text-right">
      {/* Wrapped in .sd so this matches the license links' size; the
          endpoint disclosure lives in the tooltip (data-tip — WKWebView
          never shows native title=""), keeping the
          network-endpoints-are-disclosed invariant without a caption. */}
      <span className="sd !mt-0">
        {check.isSuccess && check.data.update_available ? (
          <>
            {/* The version links to the release notes; the action installs. */}
            New version{" "}
            <a href={check.data.release_url} className="text-accent-2 hover:underline">
              {check.data.latest_version}
            </a>{" "}
            available —{" "}
            {install.isPending ? (
              `installing…${progress != null ? ` ${progress}%` : ""}`
            ) : (
              <button
                className="text-accent-2 hover:underline tip-left"
                data-tip="Downloads from github.com and objects.githubusercontent.com"
                onClick={() => install.mutate()}
              >
                Install and restart
              </button>
            )}
          </>
        ) : (
          <button
            className="text-accent-2 hover:underline disabled:opacity-60 tip-left"
            data-tip="Queries api.github.com"
            onClick={() => check.mutate()}
            disabled={check.isPending}
          >
            {label}
          </button>
        )}
      </span>
      {(install.isError || check.isError) && (
        <span className="text-xs text-red-600">
          {String(install.error ?? check.error)}
        </span>
      )}
    </div>
  );
}
