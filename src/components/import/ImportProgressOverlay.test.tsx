// @vitest-environment happy-dom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

type Handler = (e: { payload: { current: number; total: number; filename: string } }) => void;
const { listen, handlers } = vi.hoisted(() => {
  const handlers: Handler[] = [];
  return {
    handlers,
    listen: vi.fn(async (_event: string, h: Handler) => {
      handlers.push(h);
      return () => {};
    }),
  };
});
vi.mock("@tauri-apps/api/event", () => ({ listen }));

import { ImportProgressOverlay } from "./ImportProgressOverlay";
import { useLanguageStore } from "../../lib/i18n";

afterEach(() => {
  cleanup();
  handlers.length = 0;
  useLanguageStore.getState().setPref("system");
});

async function progress(current: number, total: number) {
  await waitFor(() => expect(handlers.length).toBeGreaterThan(0));
  act(() => handlers[handlers.length - 1]({ payload: { current, total, filename: "ride.fit" } }));
}

describe("ImportProgressOverlay", () => {
  it("shows the file being imported and how far along the batch is", async () => {
    render(<ImportProgressOverlay />);
    expect(screen.queryByText("Importing activities…")).toBeNull();
    await progress(2, 5);
    expect(screen.getByText("Importing activities…")).toBeTruthy();
    expect(screen.getByText("ride.fit")).toBeTruthy();
    expect(screen.getByText("2 / 5")).toBeTruthy();
  });

  it("speaks the interface language (#235)", async () => {
    useLanguageStore.getState().setPref("ru");
    render(<ImportProgressOverlay />);
    await progress(3, 12);
    expect(screen.getByText("Импорт тренировок…")).toBeTruthy();
    expect(screen.getByText("3 из 12")).toBeTruthy();
  });
});
