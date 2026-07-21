export function photoUrl(photoId: string, size: "thumb" | "full" = "full"): string {
  return `photo://localhost/${photoId}?size=${size}`;
}
