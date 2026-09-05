// HEIC/HEIF (iPhone photos) are converted to JPEG at attach time by the
// backend — macOS only (sips); other platforms report a per-file error.
export const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "webp", "heic", "heif"] as const;
export const WORKOUT_EXTENSIONS = ["gpx", "fit", "tcx"] as const;

function extOf(path: string): string {
  return path.split(/[\\/]/).pop()?.split(".").pop()?.toLowerCase() ?? "";
}

export function isImagePath(path: string): boolean {
  return (IMAGE_EXTENSIONS as readonly string[]).includes(extOf(path));
}

export function isWorkoutPath(path: string): boolean {
  const ext = extOf(path);
  if (ext === "gz") {
    // "ride.fit.gz": the backend decompresses in memory (gz_inner_ext).
    const inner = extOf(path.slice(0, -3));
    return (WORKOUT_EXTENSIONS as readonly string[]).includes(inner);
  }
  return (WORKOUT_EXTENSIONS as readonly string[]).includes(ext);
}

/** File types a drop can plausibly carry that are certainly not folders
 * and not importable — kept short on purpose: this list only decides what
 * gets the friendly "No workout files" toast instead of a backend refusal. */
const KNOWN_OTHER_EXTENSIONS = [
  "txt", "md", "csv", "json", "xml", "html", "pdf", "zip", "gz", "tar",
  "doc", "docx", "xls", "xlsx", "mp4", "mov", "mp3", "svg", "gif", "bmp",
] as const;

/**
 * A dropped path that may be a folder. The webview cannot stat a path,
 * and a folder is allowed a dot in its name ("Garmin 2.0"), so the rule
 * is not "no extension" but "no extension we know to be a file": images,
 * workouts and the common document types are files; everything else is
 * let through to the backend, which expands a folder (bounded walk) or
 * reports a plain file as unsupported.
 */
export function isFolderLikePath(path: string): boolean {
  const last = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";
  if (last.length === 0) return false;
  const ext = extOf(last);
  if (ext === "" || ext === last.toLowerCase()) return true;
  return !(
    (IMAGE_EXTENSIONS as readonly string[]).includes(ext) ||
    (WORKOUT_EXTENSIONS as readonly string[]).includes(ext) ||
    (KNOWN_OTHER_EXTENSIONS as readonly string[]).includes(ext)
  );
}

/** What the workout drop zone accepts: workout files, or a folder of them. */
export function isImportablePath(path: string): boolean {
  return isWorkoutPath(path) || isFolderLikePath(path);
}
