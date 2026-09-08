// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router";
import type { PluginInfo, ViewSpec } from "../lib/types";
import { api } from "../lib/tauri";
import { PluginPage } from "./PluginPage";

vi.mock("../lib/tauri", () => ({
  api: { getPlugins: vi.fn(), renderPluginView: vi.fn() },
}));
const mocked = vi.mocked(api);

const info = {
  id: "com.test.sync",
  name: "Garmin",
  version: "0.1.0",
  author: null,
  description: null,
  enabled: true,
  contributes: ["route.planner", "sync.source"],
  permissions: [],
  network_hosts: [],
  signed: false,
  key_fingerprint: null,
} as unknown as PluginInfo;

const view = (more: boolean): ViewSpec => ({
  title: null,
  elements: [{ type: "button", label: "Go", action: "go" }],
  continue: more ? "go" : null,
});

function renderAt(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/plugins" element={<div>PLUGINS LIST</div>} />
          <Route path="/plugin/:pluginId" element={<PluginPage />} />
          <Route path="/plugin/:pluginId/sync" element={<PluginPage point="sync.source" />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const tick = () => act(() => new Promise((r) => setTimeout(r, 20)));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("PluginPage", () => {
  it("hosts the planner without a continue loop", async () => {
    mocked.getPlugins.mockResolvedValue([info]);
    mocked.renderPluginView.mockResolvedValue(view(true));
    renderAt("/plugin/com.test.sync");
    await waitFor(() => expect(screen.getByRole("heading", { name: /Garmin/ })).toBeTruthy());
    expect(screen.queryByText(/· Sync/)).toBeNull();
    fireEvent.click(await screen.findByText("Go"));
    await waitFor(() => expect(mocked.renderPluginView).toHaveBeenCalledTimes(2));
    await tick();
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(2);
    expect(mocked.renderPluginView.mock.calls[0][1]).toBe("route.planner");
  });

  it("hosts the sync page with the loop, and leads back to the list", async () => {
    mocked.getPlugins.mockResolvedValue([info]);
    mocked.renderPluginView
      .mockResolvedValueOnce(view(false))
      .mockResolvedValueOnce(view(true))
      .mockResolvedValueOnce(view(false));
    renderAt("/plugin/com.test.sync/sync");
    await waitFor(() => expect(screen.getByText(/· Sync/)).toBeTruthy());
    fireEvent.click(await screen.findByText("Go"));
    await waitFor(() => expect(mocked.renderPluginView).toHaveBeenCalledTimes(3));
    expect(mocked.renderPluginView.mock.calls[0][1]).toBe("sync.source");
    fireEvent.click(screen.getByText("Plugins"));
    await waitFor(() => expect(screen.getByText("PLUGINS LIST")).toBeTruthy());
  });
});
