// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render } from "@testing-library/react";
import { MemoryRouter } from "react-router";

// The navbar's own text is what this is about; the import button and the
// first-run overlay bring their own queries.
vi.mock("../import/ImportDialog", () => ({ ImportDialog: () => null }));
vi.mock("./OnboardingOverlay", () => ({ OnboardingOverlay: () => null }));

import { AppShell } from "./AppShell";
import { useLanguageStore } from "../../lib/i18n";
import { useThemeStore } from "../../lib/theme";

afterEach(() => {
  cleanup();
  act(() => {
    useLanguageStore.getState().setPref("system");
    useThemeStore.getState().setMode("light");
  });
});

function renderShell(path = "/") {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AppShell>
        <p>screen</p>
      </AppShell>
    </MemoryRouter>,
  );
}

const railTitles = (container: HTMLElement) =>
  Array.from(container.querySelectorAll(".rail .ri"), (el) => el.getAttribute("title"));

describe("AppShell in the interface language (#233)", () => {
  it("names the rail, the filters, the theme switch and Settings in English", () => {
    act(() => useThemeStore.getState().setMode("light"));
    const { container, getByLabelText } = renderShell("/library");
    expect(railTitles(container)).toEqual(["Dashboard", "Calendar", "Library", "Map", "Segments"]);
    getByLabelText("Filters");
    getByLabelText("Switch to dark");
    getByLabelText("Settings");
  });

  it("names them in Russian, the theme switch by the theme it goes to", () => {
    act(() => {
      useLanguageStore.getState().setPref("ru");
      useThemeStore.getState().setMode("dark");
    });
    const { container, getByLabelText } = renderShell("/library");
    expect(railTitles(container)).toEqual(["Обзор", "Календарь", "Библиотека", "Карта", "Сегменты"]);
    expect(getByLabelText("Фильтры").getAttribute("data-tip")).toBe("Фильтры");
    fireEvent.click(getByLabelText("Включить светлую тему"));
    getByLabelText("Включить тёмную тему");
    expect(getByLabelText("Настройки").getAttribute("data-tip")).toBe("Настройки");
  });
});
