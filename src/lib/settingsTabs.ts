/** The Settings page sections. The active one lives in the URL
 * (`/settings?tab=vault`) so other pages can link straight to a section and
 * the sidebar's `/settings` match keeps working; no param means General. */
export const SETTINGS_TABS = [
  { id: "general", label: "General" },
  { id: "vault", label: "Vault" },
  { id: "garage", label: "Garage" },
  { id: "plugins", label: "Plugins" },
  { id: "about", label: "About" },
] as const;

export type SettingsTab = (typeof SETTINGS_TABS)[number]["id"];

export const DEFAULT_SETTINGS_TAB: SettingsTab = "general";

/** The tab a `?tab=` value names; anything unknown or missing is General. */
export function settingsTabFrom(param: string | null | undefined): SettingsTab {
  return SETTINGS_TABS.some((t) => t.id === param)
    ? (param as SettingsTab)
    : DEFAULT_SETTINGS_TAB;
}

/** The path that opens a Settings tab, as compact as possible. */
export function settingsPath(tab: SettingsTab): string {
  return tab === DEFAULT_SETTINGS_TAB ? "/settings" : `/settings?tab=${tab}`;
}

/** The tab an arrow/Home/End key moves to from `current`, or null for any
 * other key. Left/Right wrap around, as the ARIA tabs pattern expects. */
export function nextSettingsTab(current: SettingsTab, key: string): SettingsTab | null {
  const ids = SETTINGS_TABS.map((t) => t.id);
  const i = ids.indexOf(current);
  switch (key) {
    case "ArrowRight":
      return ids[(i + 1) % ids.length];
    case "ArrowLeft":
      return ids[(i - 1 + ids.length) % ids.length];
    case "Home":
      return ids[0];
    case "End":
      return ids[ids.length - 1];
    default:
      return null;
  }
}
