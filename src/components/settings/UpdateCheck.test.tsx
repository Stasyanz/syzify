// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { act, render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import type { UpdateCheck as UpdateCheckResult } from "../../lib/types";
import { UpdateCheck } from "./UpdateCheck";

vi.mock("../../lib/tauri", () => ({
  api: { checkForUpdates: vi.fn(), installUpdate: vi.fn() },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const mocked = vi.mocked(api);

function result(over: Partial<UpdateCheckResult> = {}): UpdateCheckResult {
  return {
    current_version: "0.1.1",
    latest_version: "0.1.1",
    update_available: false,
    release_url: "https://github.com/Stasyanz/syzify/releases",
    ...over,
  };
}

afterEach(cleanup);
beforeEach(() => vi.clearAllMocks());

function renderRow() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <UpdateCheck />
    </QueryClientProvider>,
  );
}

describe("UpdateCheck", () => {
  it("does not touch the network until clicked, and discloses the endpoint", () => {
    renderRow();
    expect(mocked.checkForUpdates).not.toHaveBeenCalled();
    expect(mocked.installUpdate).not.toHaveBeenCalled();
    // Disclosure lives in the tooltip, not a permanent caption.
    expect(
      screen.getByText("Check for updates").getAttribute("data-tip"),
    ).toContain("api.github.com");
  });

  it("replaces the button label with the up-to-date answer, still re-checkable", async () => {
    mocked.checkForUpdates.mockResolvedValue(result());
    renderRow();
    fireEvent.click(screen.getByText("Check for updates"));
    const btn = await screen.findByText("You're up to date");
    // Same element morphed — no second line appended.
    expect(btn.tagName).toBe("BUTTON");
    expect(screen.queryByText("Check for updates")).toBeNull();
    // Still clickable for a re-check.
    fireEvent.click(btn);
    await waitFor(() => expect(mocked.checkForUpdates).toHaveBeenCalledTimes(2));
  });

  it("offers a one-click install for a newer version", async () => {
    mocked.checkForUpdates.mockResolvedValue(
      result({
        update_available: true,
        latest_version: "0.2.0",
        release_url: "https://github.com/Stasyanz/syzify/releases/tag/v0.2.0",
      }),
    );
    // Never resolves: on success the backend restarts the app instead.
    mocked.installUpdate.mockReturnValue(new Promise(() => {}));
    renderRow();
    fireEvent.click(screen.getByText("Check for updates"));
    const install = await screen.findByText("Install and restart");
    // Nothing downloads until the second explicit click, and the download
    // endpoint is disclosed the same way as the check endpoint.
    expect(mocked.installUpdate).not.toHaveBeenCalled();
    expect(install.getAttribute("data-tip")).toContain("github.com");
    // Release notes stay reachable through the version link.
    const link = screen.getByText("0.2.0") as HTMLAnchorElement;
    expect(link.getAttribute("href")).toContain("/releases/tag/v0.2.0");
    fireEvent.click(install);
    await waitFor(() => expect(mocked.installUpdate).toHaveBeenCalledTimes(1));
    // In-place morph while the download runs — no appended lines.
    expect(await screen.findByText(/installing…/)).toBeTruthy();
    expect(screen.queryByText("Install and restart")).toBeNull();
  });

  it("renders download progress from update:progress events", async () => {
    mocked.checkForUpdates.mockResolvedValue(
      result({ update_available: true, latest_version: "0.2.0" }),
    );
    type ProgressEvent = { payload: { downloaded: number; total: number | null } };
    let handler: ((e: ProgressEvent) => void) | undefined;
    vi.mocked(listen).mockImplementationOnce(async (_event, cb) => {
      handler = cb as unknown as (e: ProgressEvent) => void;
      return () => {};
    });
    mocked.installUpdate.mockReturnValue(new Promise(() => {}));
    renderRow();
    fireEvent.click(screen.getByText("Check for updates"));
    fireEvent.click(await screen.findByText("Install and restart"));
    await screen.findByText(/installing…/);
    // Total still unknown → no percentage yet.
    act(() => handler!({ payload: { downloaded: 10, total: null } }));
    expect(screen.queryByText(/%/)).toBeNull();
    act(() => handler!({ payload: { downloaded: 50, total: 200 } }));
    expect(await screen.findByText(/installing… 25%/)).toBeTruthy();
    // Never claims 100% — the backend restarts before "done" could render.
    act(() => handler!({ payload: { downloaded: 200, total: 200 } }));
    expect(await screen.findByText(/installing… 99%/)).toBeTruthy();
  });

  it("surfaces a failed install and keeps the button for a retry", async () => {
    mocked.checkForUpdates.mockResolvedValue(
      result({ update_available: true, latest_version: "0.2.0" }),
    );
    mocked.installUpdate.mockRejectedValue("Update install failed: bad signature");
    renderRow();
    fireEvent.click(screen.getByText("Check for updates"));
    fireEvent.click(await screen.findByText("Install and restart"));
    expect(await screen.findByText(/bad signature/)).toBeTruthy();
    expect(screen.getByText("Install and restart")).toBeTruthy();
  });

  it("surfaces a failed check", async () => {
    mocked.checkForUpdates.mockRejectedValue("Update check failed: offline");
    renderRow();
    fireEvent.click(screen.getByText("Check for updates"));
    expect(await screen.findByText(/offline/)).toBeTruthy();
  });
});
