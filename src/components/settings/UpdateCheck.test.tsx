// @vitest-environment happy-dom
import { describe, it, expect, afterEach, beforeEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import type { UpdateCheck as UpdateCheckResult } from "../../lib/types";
import { UpdateCheck } from "./UpdateCheck";

vi.mock("../../lib/tauri", () => ({
  api: { checkForUpdates: vi.fn() },
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

  it("offers the release link for a newer version", async () => {
    mocked.checkForUpdates.mockResolvedValue(
      result({
        update_available: true,
        latest_version: "0.2.0",
        release_url: "https://github.com/Stasyanz/syzify/releases/tag/v0.2.0",
      }),
    );
    renderRow();
    fireEvent.click(screen.getByText("Check for updates"));
    expect(await screen.findByText(/New version 0\.2\.0 available/)).toBeTruthy();
    const link = screen.getByText("Download") as HTMLAnchorElement;
    expect(link.getAttribute("href")).toContain("/releases/tag/v0.2.0");
  });

  it("surfaces a failed check", async () => {
    mocked.checkForUpdates.mockRejectedValue("Update check failed: offline");
    renderRow();
    fireEvent.click(screen.getByText("Check for updates"));
    expect(await screen.findByText(/offline/)).toBeTruthy();
  });
});
