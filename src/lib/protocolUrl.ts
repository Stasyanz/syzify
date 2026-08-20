import { convertFileSrc } from "@tauri-apps/api/core";

/** Base URL (with trailing slash) for a Tauri custom protocol. macOS serves
 * them as `<proto>://localhost/`, while WebView2 on Windows only allows
 * `http://<proto>.localhost/` — convertFileSrc picks the platform's form at
 * runtime. Outside Tauri (vitest, plain browser) fall back to the macOS
 * form so URL-building code keeps working. */
export function protocolBase(protocol: string): string {
  try {
    return convertFileSrc("", protocol);
  } catch {
    return `${protocol}://localhost/`;
  }
}
