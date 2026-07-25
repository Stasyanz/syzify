import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Upload } from "lucide-react";
import { api } from "../../lib/tauri";
import { useToastStore } from "../../stores/toastStore";
import { invalidateActivityData } from "../../lib/activityInvalidation";

/** "icon" → compact navbar button; "button" → full accent CTA (empty state). */
export function ImportDialog({ variant = "button" }: { variant?: "icon" | "button" }) {
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);

  const importMutation = useMutation({
    mutationFn: (paths: string[]) => api.importFiles(paths),
    onSuccess: (data) => {
      invalidateActivityData(queryClient);
      const parts = [`Imported ${data.imported} activit${data.imported === 1 ? "y" : "ies"}`];
      if (data.skipped > 0) parts.push(`skipped ${data.skipped} (duplicates)`);
      if (data.failed.length > 0) parts.push(`${data.failed.length} failed`);
      addToast(data.failed.length > 0 ? "warning" : "success", parts.join(", "));
    },
    onError: (error) => {
      addToast("error", `Import failed: ${error.message ?? "Unknown error"}`);
    },
  });

  const handleImport = useCallback(async () => {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Workout files", extensions: ["gpx", "fit", "tcx", "gz"] }],
    });
    if (selected && selected.length > 0) importMutation.mutate(selected);
  }, [importMutation]);

  if (variant === "icon") {
    return (
      <button
        className="themebtn"
        onClick={handleImport}
        disabled={importMutation.isPending}
        data-tip="Import workout files"
        aria-label="Import workout files"
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
      {importMutation.isPending ? "Importing..." : "Import Files"}
    </button>
  );
}
