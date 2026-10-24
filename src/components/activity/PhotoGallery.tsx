import { useCallback, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { confirmDialog } from "../../stores/confirmStore";
import { moveItem } from "../../lib/reorder";
import { ImagePlus, Trash2, Share2, X, Pencil } from "lucide-react";
import { api } from "../../lib/tauri";
import { useToastStore } from "../../stores/toastStore";
import type { Photo } from "../../lib/types";
import { photoUrl } from "./photoUrl";
import { errorText } from "../../lib/errors";
import { capitalize, formatNumber, useT } from "../../lib/i18n";

interface Props {
  activityId: string;
  onShare: (photo: Photo) => void;
}

export function PhotoGallery({ activityId, onShare }: Props) {
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const t = useT();
  const [lightbox, setLightbox] = useState<Photo | null>(null);
  const [editingCaption, setEditingCaption] = useState<string | null>(null);
  const [captionDraft, setCaptionDraft] = useState("");
  const [dragIndex, setDragIndex] = useState<number | null>(null);

  const { data: photos = [] } = useQuery({
    queryKey: ["photos", activityId],
    queryFn: () => api.getPhotos(activityId),
  });

  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["photos", activityId] });

  const attachMutation = useMutation({
    mutationFn: (paths: string[]) => api.attachPhotos(activityId, paths),
    onSuccess: (res) => {
      invalidate();
      const parts: string[] = [];
      if (res.attached.length) parts.push(t("activity.photos.added", { count: res.attached.length }));
      if (res.skipped.length) parts.push(t("activity.photos.skipped", { count: res.skipped.length }));
      if (res.failed.length) parts.push(t("activity.photos.failed", { count: res.failed.length }));
      const tone = res.failed.length > 0 ? "warning" : "success";
      // The parts after the first are written in lower case; whichever
      // comes first opens the sentence.
      if (parts.length) addToast(tone, capitalize(parts.join(", ")));
    },
    onError: (e: unknown) => addToast("error", t("activity.photos.attachFailed", { error: errorText(e) })),
  });

  const deleteMutation = useMutation({
    mutationFn: (photoId: string) => api.deletePhoto(photoId),
    onSuccess: () => invalidate(),
    onError: (e: unknown) => addToast("error", errorText(e)),
  });

  const captionMutation = useMutation({
    mutationFn: (args: { photoId: string; caption: string | null }) =>
      api.updatePhotoCaption(args.photoId, args.caption),
    onSuccess: () => {
      invalidate();
      setEditingCaption(null);
    },
    onError: (e: unknown) => addToast("error", errorText(e)),
  });

  const reorderMutation = useMutation({
    mutationFn: (ids: string[]) => api.reorderPhotos(ids),
    onError: (e: unknown) => {
      addToast("error", t("activity.photos.reorderFailed", { error: errorText(e) }));
      invalidate();
    },
  });

  const handleItemDrop = useCallback(
    (targetIdx: number) => {
      setDragIndex(null);
      if (dragIndex === null || dragIndex === targetIdx) return;
      const next = moveItem(photos, dragIndex, targetIdx);
      // Optimistically reflect the new order, then persist.
      queryClient.setQueryData(["photos", activityId], next);
      reorderMutation.mutate(next.map((p) => p.id));
    },
    [dragIndex, photos, queryClient, activityId, reorderMutation]
  );

  const handleAdd = useCallback(async () => {
    const selected = await open({
      multiple: true,
      filters: [{ name: t("activity.photos.images"), extensions: ["jpg", "jpeg", "png", "webp", "heic", "heif"] }],
    });
    if (selected && selected.length > 0) {
      attachMutation.mutate(selected);
    }
  }, [attachMutation, t]);

  // Native OS drops anywhere on the activity page attach photos — that is
  // handled window-wide by useDropImport, not here.
  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-sm font-medium text-muted">
          {t("activity.photos.title")}{" "}
          {photos.length > 0 && <span className="text-faint">({formatNumber(photos.length)})</span>}
        </h3>
        <button
          onClick={handleAdd}
          disabled={attachMutation.isPending}
          className="flex items-center gap-1.5 text-sm text-muted hover:text-ink px-3 py-1.5 rounded border border-border hover:border-border-2 disabled:opacity-50"
        >
          <ImagePlus size={14} />
          {t(attachMutation.isPending ? "activity.photos.adding" : "activity.photos.add")}
        </button>
      </div>

      <div className="border rounded-lg p-3 border-border">
        {photos.length === 0 ? (
          <div className="text-center text-sm text-faint py-8">{t("activity.photos.empty")}</div>
        ) : (
          <div className="grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 gap-2">
            {photos.map((p, idx) => (
              <div
                key={p.id}
                draggable
                onDragStart={() => setDragIndex(idx)}
                onDragOver={(e) => {
                  if (dragIndex !== null) e.preventDefault();
                }}
                onDrop={(e) => {
                  if (dragIndex !== null) {
                    e.preventDefault();
                    e.stopPropagation();
                    handleItemDrop(idx);
                  }
                }}
                onDragEnd={() => setDragIndex(null)}
                className={`relative group rounded overflow-hidden bg-card-2 aspect-square cursor-move ${
                  dragIndex === idx ? "opacity-40" : ""
                }`}
              >
                <img
                  src={photoUrl(p.id, "thumb")}
                  alt={p.caption ?? ""}
                  className="w-full h-full object-cover cursor-pointer"
                  onClick={() => setLightbox(p)}
                />
                <div className="absolute inset-x-0 top-0 p-1 flex justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    onClick={() => onShare(p)}
                    title={t("common.share")}
                    className="p-1 rounded bg-black/60 hover:bg-black/80 text-white"
                  >
                    <Share2 size={12} />
                  </button>
                  <button
                    onClick={() => {
                      setEditingCaption(p.id);
                      setCaptionDraft(p.caption ?? "");
                    }}
                    title={t("activity.photos.editCaption")}
                    className="p-1 rounded bg-black/60 hover:bg-black/80 text-white"
                  >
                    <Pencil size={12} />
                  </button>
                  <button
                    onClick={async () => {
                      const ok = await confirmDialog({
                        title: t("activity.photos.deleteTitle"),
                        message: t("activity.photos.deleteMessage"),
                        confirmLabel: t("common.delete"),
                        danger: true,
                      });
                      if (ok) deleteMutation.mutate(p.id);
                    }}
                    title={t("common.delete")}
                    className="p-1 rounded bg-black/60 hover:bg-red-700 text-white"
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
                {p.caption && (
                  <div className="absolute inset-x-0 bottom-0 p-1 text-xs text-white bg-gradient-to-t from-black/80 to-transparent truncate">
                    {p.caption}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {editingCaption && (
        // No backdrop-click close (app-wide modal policy): a stray click
        // must not discard a half-typed caption. Closing is explicit.
        <div className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/50">
          <div className="bg-card rounded-lg p-4 w-96">
            <h3 className="text-sm font-medium mb-2">{t("activity.photos.editCaption")}</h3>
            <input
              type="text"
              value={captionDraft}
              onChange={(e) => setCaptionDraft(e.target.value)}
              className="w-full border border-border-2 rounded px-2 py-1.5 text-sm"
              autoFocus
            />
            <div className="flex justify-end gap-2 mt-3">
              <button
                onClick={() => setEditingCaption(null)}
                className="text-sm text-muted px-3 py-1.5"
              >
                {t("common.cancel")}
              </button>
              <button
                onClick={() =>
                  captionMutation.mutate({
                    photoId: editingCaption,
                    caption: captionDraft.trim() === "" ? null : captionDraft.trim(),
                  })
                }
                className="text-sm bg-accent text-white px-3 py-1.5 rounded"
              >
                {t("common.save")}
              </button>
            </div>
          </div>
        </div>
      )}

      {lightbox && (
        <div
          className="fixed inset-0 z-[9999] flex items-center justify-center bg-black/90"
          onClick={() => setLightbox(null)}
        >
          <button
            onClick={() => setLightbox(null)}
            aria-label={t("common.close")}
            className="absolute top-4 right-4 p-2 rounded bg-black/60 hover:bg-black/80 text-white"
          >
            <X size={20} />
          </button>
          <img
            src={photoUrl(lightbox.id, "full")}
            alt={lightbox.caption ?? ""}
            className="max-w-[95vw] max-h-[95vh] object-contain"
            onClick={(e) => e.stopPropagation()}
          />
        </div>
      )}
    </div>
  );
}
