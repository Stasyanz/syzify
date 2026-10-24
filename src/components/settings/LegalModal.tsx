import { useQuery } from "@tanstack/react-query";
import { X, Loader2 } from "lucide-react";
import { api } from "../../lib/tauri";
import { errorText } from "../../lib/errors";
import { useT } from "../../lib/i18n";

export type LegalDoc = "license" | "exception" | "notices";

/** The legal documents are English, and so are the names of the licence
 * and its exception: a translated title would name a document that does
 * not exist. The notices are a list, named in the interface language. */
const LEGAL_NAMES = {
  license: "GNU Affero General Public License v3",
  exception: "Syzify Plugin Exception",
} as const;

/** Full text of a bundled legal document (Settings → About). Only the
 * modal's chrome is translated; the text is shown as it is written. */
export function LegalModal({ doc, onClose }: { doc: LegalDoc; onClose: () => void }) {
  const t = useT();
  const { data, error, isLoading } = useQuery({
    queryKey: ["legalText", doc],
    queryFn: () => api.getLegalText(doc),
    staleTime: Infinity,
  });

  return (
    // No backdrop-click close (app-wide modal policy) — closing is explicit.
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="bg-card rounded-xl shadow-2xl w-full max-w-2xl mx-4 p-6 flex flex-col max-h-[80vh]">
        <div className="flex items-center justify-between mb-3">
          {doc === "notices" ? (
            <h2 className="text-lg font-semibold">{t("settings.about.notices")}</h2>
          ) : (
            <h2 className="text-lg font-semibold" translate="no">
              {LEGAL_NAMES[doc]}
            </h2>
          )}
          <button
            onClick={onClose}
            className="text-faint hover:text-muted"
            aria-label={t("common.close")}
          >
            <X size={18} />
          </button>
        </div>
        {isLoading ? (
          <div className="flex items-center justify-center py-10 text-faint">
            <Loader2 size={18} className="animate-spin" />
          </div>
        ) : error ? (
          <p className="text-sm text-red-500">{errorText(error)}</p>
        ) : (
          <pre
            className="text-xs font-mono whitespace-pre-wrap overflow-y-auto scroll-themed flex-1 text-muted"
            lang="en"
            translate="no"
          >
            {data}
          </pre>
        )}
      </div>
    </div>
  );
}
