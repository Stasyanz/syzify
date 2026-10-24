import { describe, it, expect } from "vitest";
import { SETTINGS_TABS, nextSettingsTab, settingsPath, settingsTabFrom } from "./settingsTabs";
import { translate } from "./i18n";

describe("settingsTabFrom", () => {
  it("names every tab and falls back to General otherwise", () => {
    for (const t of SETTINGS_TABS) expect(settingsTabFrom(t.id)).toBe(t.id);
    expect(settingsTabFrom(null)).toBe("general");
    expect(settingsTabFrom(undefined)).toBe("general");
    expect(settingsTabFrom("")).toBe("general");
    expect(settingsTabFrom("nope")).toBe("general");
    // Exact ids only: no case folding, no prefix matching.
    expect(settingsTabFrom("Vault")).toBe("general");
  });
});

describe("settingsPath", () => {
  it("keeps the default tab off the URL and round-trips the others", () => {
    expect(settingsPath("general")).toBe("/settings");
    expect(settingsPath("plugins")).toBe("/settings?tab=plugins");
    for (const t of SETTINGS_TABS) {
      const query = new URL(settingsPath(t.id), "http://x").searchParams.get("tab");
      expect(settingsTabFrom(query)).toBe(t.id);
    }
  });
});

describe("SETTINGS_TABS", () => {
  it("names each tab by a message, in the order of the tab bar", () => {
    expect(SETTINGS_TABS.map((tab) => [tab.id, translate("en", tab.labelKey), translate("ru", tab.labelKey)])).toEqual([
      ["general", "General", "Общие"],
      ["vault", "Vault", "Хранилище"],
      ["garage", "Garage", "Гараж"],
      ["plugins", "Plugins", "Плагины"],
      ["about", "About", "О программе"],
    ]);
  });
});

describe("nextSettingsTab", () => {
  it("cycles with the arrows, jumps with Home/End and ignores other keys", () => {
    expect(nextSettingsTab("general", "ArrowRight")).toBe("vault");
    expect(nextSettingsTab("vault", "ArrowRight")).toBe("garage");
    expect(nextSettingsTab("about", "ArrowRight")).toBe("general");
    expect(nextSettingsTab("general", "ArrowLeft")).toBe("about");
    expect(nextSettingsTab("plugins", "Home")).toBe("general");
    expect(nextSettingsTab("plugins", "End")).toBe("about");
    expect(nextSettingsTab("plugins", "Enter")).toBeNull();
    expect(nextSettingsTab("plugins", "ArrowDown")).toBeNull();
  });
});
