import { useQuery } from "@tanstack/react-query";
import { X, Loader2 } from "lucide-react";
import { api } from "../../lib/tauri";

export type LegalDoc = "license" | "exception" | "notices";

const TITLES: Record<LegalDoc, string> = {
  license: "GNU Affero General Public License v3",
  exception: "Syzify Plugin Exception",
  notices: "Third-party notices",
};

/** Full text of a bundled legal document (Settings → About). */
export function LegalModal({ doc, onClose }: { doc: LegalDoc; onClose: () => void }) {
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
          <h2 className="text-lg font-semibold">{TITLES[doc]}</h2>
          <button
            onClick={onClose}
            className="text-faint hover:text-muted"
            aria-label="Close"
          >
            <X size={18} />
          </button>
        </div>
        {isLoading ? (
          <div className="flex items-center justify-center py-10 text-faint">
            <Loader2 size={18} className="animate-spin" />
          </div>
        ) : error ? (
          <p className="text-sm text-red-500">{String(error)}</p>
        ) : (
          <pre className="text-xs font-mono whitespace-pre-wrap overflow-y-auto scroll-themed flex-1 text-muted">
            {data}
          </pre>
        )}
      </div>
    </div>
  );
}
