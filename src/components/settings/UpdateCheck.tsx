import { useMutation } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { capitalize, formatPercent, useRichT, useT } from "../../lib/i18n";
import { useBusy, useBusyError, useBusyProgress, useBusyStore } from "../../stores/busyStore";

/** Manual "Check for updates" under the version in Settings → About, and
 * (centered) on the vault-too-new boot error screen — both commands it calls
 * are vault-independent, so it works before the DB opens. Strictly
 * user-initiated (the app never phones home on its own) with the
 * endpoint disclosed right on the row; an available update offers a one-click
 * signed install — the download still only starts on an explicit click.
 * The install and its progress live in the busy store: a remount mid-download
 * (a language change) shows them again instead of a fresh Check button. */
export function UpdateCheck({ centered = false }: { centered?: boolean }) {
  const t = useT();
  const rt = useRichT();
  const check = useMutation({ mutationFn: () => api.checkForUpdates() });
  const installing = useBusy("updateInstall");
  const progress = useBusyProgress("updateInstall");
  // In the store too: a row remounted while the download ran still says
  // why it failed.
  const installFailure = useBusyError("updateInstall");

  // Success never lands: the backend restarts the app after installing.
  async function install() {
    const ops = useBusyStore.getState();
    ops.start("updateInstall");
    try {
      const unlisten = await listen<{ downloaded: number; total: number | null }>(
        "update:progress",
        (e) => {
          const { downloaded, total } = e.payload;
          if (total) ops.progress("updateInstall", Math.min(99, Math.floor((downloaded / total) * 100)));
        },
      );
      try {
        await api.installUpdate();
      } finally {
        unlisten();
      }
      ops.finish("updateInstall");
    } catch (e) {
      ops.finish("updateInstall", e);
    }
  }

  const installingText =
    progress != null
      ? t("settings.update.installingPercent", { percent: formatPercent(progress, 0) })
      : t("settings.update.installing");
  const upToDate = check.isSuccess && !check.data.update_available;
  // One line that changes in place — never a second appended line.
  const label = check.isPending
    ? t("settings.update.checking")
    : upToDate
      ? t("settings.update.upToDate")
      : t("settings.update.check");
  // An install failure stays said until the next install starts, as the
  // check's own failure does until the next check.
  const failed = installFailure.failed || check.isError;
  const error = installFailure.failed ? installFailure.error : check.error;

  return (
    <div
      className={`flex flex-col gap-0.5 ${centered ? "items-center text-center" : "items-end text-right"}`}
    >
      {/* Wrapped in .sd so this matches the license links' size; the
          endpoint disclosure lives in the tooltip (data-tip — WKWebView
          never shows native title=""), keeping the
          network-endpoints-are-disclosed invariant without a caption. */}
      <span className="sd !mt-0">
        {check.isSuccess && check.data.update_available ? (
          // The version links to the release notes; the action installs.
          rt("settings.update.available", {
            version: (
              <a href={check.data.release_url} className="text-accent-2 hover:underline">
                {check.data.latest_version}
              </a>
            ),
            action: installing ? (
              installingText
            ) : (
              <button
                className="text-accent-2 hover:underline tip-left"
                data-tip={t("settings.update.installTip")}
                onClick={() => void install()}
              >
                {t("settings.update.install")}
              </button>
            ),
          })
        ) : installing ? (
          // Remounted mid-install: the check that found the version is gone,
          // the download is not. Alone on its line, so it opens a sentence.
          capitalize(installingText)
        ) : (
          <button
            className="text-accent-2 hover:underline disabled:opacity-60 tip-left"
            data-tip={t("settings.update.checkTip")}
            onClick={() => check.mutate()}
            disabled={check.isPending}
          >
            {label}
          </button>
        )}
      </span>
      {failed && <span className="text-xs text-red-600">{errorText(error)}</span>}
    </div>
  );
}
