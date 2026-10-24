import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Upload } from "lucide-react";
import { api } from "../../lib/tauri";
import { useToastStore } from "../../stores/toastStore";
import { invalidateActivityData } from "../../lib/activityInvalidation";
import { formatImportSummary } from "../../lib/importSummary";
import { errorText } from "../../lib/errors";
import { useT } from "../../lib/i18n";

/** "icon" → compact navbar button; "button" → full accent CTA (empty state). */
export function ImportDialog({ variant = "button" }: { variant?: "icon" | "button" }) {
  const t = useT();
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);

  const importMutation = useMutation({
    mutationFn: (paths: string[]) => api.importFiles(paths),
    onSuccess: (data) => {
      invalidateActivityData(queryClient);
      const { level, text } = formatImportSummary(data, {}, t);
      addToast(level, text);
    },
    onError: (error) => {
      addToast("error", t("common.importFailed", { error: errorText(error) }));
    },
  });

  const handleImport = useCallback(async () => {
    const selected = await open({
      multiple: true,
      filters: [{ name: t("import.dialog.filter"), extensions: ["gpx", "fit", "tcx", "gz"] }],
    });
    if (selected && selected.length > 0) importMutation.mutate(selected);
  }, [importMutation, t]);

  if (variant === "icon") {
    return (
      <button
        // The onboarding overlay anchors its spotlight + arrow to this id.
        id="import-workouts-btn"
        className="themebtn"
        onClick={handleImport}
        disabled={importMutation.isPending}
        data-tip={t("import.dialog.tip")}
        aria-label={t("import.dialog.tip")}
      >
        <Upload size={18} className={importMutation.isPending ? "animate-pulse" : ""} />
      </button>
    );
  }

  return (
    <button
      onClick={handleImport}
      disabled={importMutation.isPending}
      className="bg-accent hover:bg-accent-2 text-white px-4 py-2 rounded-lg font-medium transition-colors disabled:opacity-60"
    >
      {importMutation.isPending ? t("common.importing") : t("import.dialog.button")}
    </button>
  );
}
