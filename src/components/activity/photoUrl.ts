import { protocolBase } from "../../lib/protocolUrl";

export function photoUrl(photoId: string, size: "thumb" | "full" = "full"): string {
  return `${protocolBase("photo")}${photoId}?size=${size}`;
}
