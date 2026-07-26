import { useCallback, useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { confirmDialog } from "../../stores/confirmStore";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { isImagePath } from "../../lib/fileTypes";
import { moveItem } from "../../lib/reorder";
import { ImagePlus, Trash2, Share2, X, Pencil } from "lucide-react";
import { api } from "../../lib/tauri";
import { useToastStore } from "../../stores/toastStore";
import type { Photo } from "../../lib/types";
import { photoUrl } from "./photoUrl";

interface Props {
  activityId: string;
  onShare: (photo: Photo) => void;
}

export function PhotoGallery({ activityId, onShare }: Props) {
  const queryClient = useQueryClient();
  const addToast = useToastStore((s) => s.addToast);
  const [lightbox, setLightbox] = useState<Photo | null>(null);
  const [editingCaption, setEditingCaption] = useState<string | null>(null);
  const [captionDraft, setCaptionDraft] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const dropZoneRef = useRef<HTMLDivElement>(null);

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
      if (res.attached.length) parts.push(`Added ${res.attached.length}`);
      if (res.skipped.length) parts.push(`skipped ${res.skipped.length} duplicates`);
      if (res.failed.length) parts.push(`${res.failed.length} failed`);
      const tone = res.failed.length > 0 ? "warning" : "success";
      if (parts.length) addToast(tone, parts.join(", "));
    },
    onError: (e: Error) => addToast("error", `Failed to attach photos: ${e.message}`),
  });

  const deleteMutation = useMutation({
    mutationFn: (photoId: string) => api.deletePhoto(photoId),
    onSuccess: () => invalidate(),
    onError: (e: Error) => addToast("error", e.message),
  });

  const captionMutation = useMutation({
    mutationFn: (args: { photoId: string; caption: string | null }) =>
      api.updatePhotoCaption(args.photoId, args.caption),
    onSuccess: () => {
      invalidate();
      setEditingCaption(null);
    },
    onError: (e: Error) => addToast("error", e.message),
  });

  const reorderMutation = useMutation({
    mutationFn: (ids: string[]) => api.reorderPhotos(ids),
    onError: (e: Error) => {
      addToast("error", `Failed to reorder: ${e.message}`);
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
      filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png", "webp"] }],
    });
    if (selected && selected.length > 0) {
      attachMutation.mutate(selected);
    }
  }, [attachMutation]);

  // Native OS file drops are delivered by Tauri (HTML5 file drops are
  // suppressed when dragDropEnabled is on). We only react to drops that land
  // over this gallery's drop zone.
  useEffect(() => {
    const isOverZone = (pos: { x: number; y: number }) => {
      const el = dropZoneRef.current;
      if (!el) return false;
      const r = el.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const x = pos.x / dpr;
      const y = pos.y / dpr;
      return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
    };
    let unlisten: (() => void) | undefined;
    let active = true;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const p = event.payload;
        if (p.type === "over") {
          setDragOver(isOverZone(p.position));
        } else if (p.type === "leave") {
          setDragOver(false);
        } else if (p.type === "drop") {
          setDragOver(false);
          if (!isOverZone(p.position)) return;
          const images = p.paths.filter(isImagePath);
          if (images.length > 0) {
            attachMutation.mutate(images);
          } else if (p.paths.length > 0) {
            addToast("info", "Only JPG, PNG or WebP images can be attached");
          }
        }
      })
      .then((u) => {
        if (active) unlisten = u;
        else u();
      });
    return () => {
      active = false;
      unlisten?.();
    };
  }, [attachMutation, addToast]);

  return (
    <div>
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-sm font-medium text-muted">
          Photos {photos.length > 0 && <span className="text-faint">({photos.length})</span>}
        </h3>
        <button
          onClick={handleAdd}
          disabled={attachMutation.isPending}
          className="flex items-center gap-1.5 text-sm text-muted hover:text-ink px-3 py-1.5 rounded border border-border hover:border-border-2 disabled:opacity-50"
        >
          <ImagePlus size={14} />
          {attachMutation.isPending ? "Adding..." : "Add Photos"}
        </button>
      </div>

      <div
        ref={dropZoneRef}
        data-photo-dropzone
        className={`border rounded-lg p-3 transition-colors ${
          dragOver ? "border-accent bg-accent-soft" : "border-border"
        }`}
      >
        {photos.length === 0 ? (
          <div className="text-center text-sm text-faint py-8">
            No photos yet. Click "Add Photos" to attach images to this activity.
          </div>
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
                    title="Share"
                    className="p-1 rounded bg-black/60 hover:bg-black/80 text-white"
                  >
                    <Share2 size={12} />
                  </button>
                  <button
                    onClick={() => {
                      setEditingCaption(p.id);
                      setCaptionDraft(p.caption ?? "");
                    }}
                    title="Edit caption"
                    className="p-1 rounded bg-black/60 hover:bg-black/80 text-white"
                  >
                    <Pencil size={12} />
                  </button>
                  <button
                    onClick={async () => {
                      const ok = await confirmDialog({
                        title: "Delete photo",
                        message: "Delete this photo?",
                        confirmLabel: "Delete",
                        danger: true,
                      });
                      if (ok) deleteMutation.mutate(p.id);
                    }}
                    title="Delete"
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
            <h3 className="text-sm font-medium mb-2">Edit caption</h3>
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
                Cancel
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
                Save
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
