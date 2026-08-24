import { useMutation } from "@tanstack/react-query";
import { api } from "../../lib/tauri";

/** Manual "Check for updates" under the version in Settings → General.
 * Strictly user-initiated (the app never phones home on its own) with the
 * endpoint disclosed right on the row; an available update links to the
 * release page — installing stays in the user's hands. */
export function UpdateCheck() {
  const check = useMutation({ mutationFn: () => api.checkForUpdates() });

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
            New version {check.data.latest_version} available —{" "}
            <a href={check.data.release_url} className="text-accent-2 hover:underline">
              Download
            </a>
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
      {check.isError && (
        <span className="text-xs text-red-600">{String(check.error)}</span>
      )}
    </div>
  );
}
