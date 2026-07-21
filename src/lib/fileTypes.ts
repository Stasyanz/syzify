export const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png", "webp"] as const;
export const WORKOUT_EXTENSIONS = ["gpx", "fit", "tcx"] as const;

function extOf(path: string): string {
  return path.split(/[\\/]/).pop()?.split(".").pop()?.toLowerCase() ?? "";
}

export function isImagePath(path: string): boolean {
  return (IMAGE_EXTENSIONS as readonly string[]).includes(extOf(path));
}

export function isWorkoutPath(path: string): boolean {
  return (WORKOUT_EXTENSIONS as readonly string[]).includes(extOf(path));
}
