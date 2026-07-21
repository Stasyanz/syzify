// @vitest-environment happy-dom
import { describe, it, expect, beforeEach } from "vitest";
import {
  resolveDark,
  readStoredMode,
  applyTheme,
  useThemeStore,
} from "./theme";

beforeEach(() => {
  localStorage.clear();
  document.documentElement.classList.remove("dark");
  // reset store to a known state without touching persistence assertions
  useThemeStore.setState({ mode: "system" });
});

describe("resolveDark", () => {
  it("is dark when mode=dark regardless of OS", () => {
    expect(resolveDark("dark", false)).toBe(true);
    expect(resolveDark("dark", true)).toBe(true);
  });

  it("is light when mode=light regardless of OS", () => {
    expect(resolveDark("light", false)).toBe(false);
    expect(resolveDark("light", true)).toBe(false);
  });

  it("follows the OS when mode=system", () => {
    expect(resolveDark("system", true)).toBe(true);
    expect(resolveDark("system", false)).toBe(false);
  });
});

describe("readStoredMode", () => {
  it("defaults to system when unset", () => {
    expect(readStoredMode()).toBe("system");
  });

  it("defaults to system when value is garbage", () => {
    localStorage.setItem("syzify-theme", "purple");
    expect(readStoredMode()).toBe("system");
  });

  it("reads a valid stored value", () => {
    localStorage.setItem("syzify-theme", "dark");
    expect(readStoredMode()).toBe("dark");
  });
});

describe("applyTheme", () => {
  it("adds .dark for dark mode and removes it for light", () => {
    applyTheme("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    applyTheme("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });
});

describe("useThemeStore.setMode", () => {
  it("persists the choice and applies the class", () => {
    useThemeStore.getState().setMode("dark");
    expect(localStorage.getItem("syzify-theme")).toBe("dark");
    expect(useThemeStore.getState().mode).toBe("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    useThemeStore.getState().setMode("light");
    expect(localStorage.getItem("syzify-theme")).toBe("light");
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });
});
