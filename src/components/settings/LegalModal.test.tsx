// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { api } from "../../lib/tauri";
import { LegalModal, type LegalDoc } from "./LegalModal";

vi.mock("../../lib/tauri", () => ({
  api: {
    getLegalText: vi.fn(),
  },
  isTauri: () => false,
}));

afterEach(cleanup);

function renderModal(doc: LegalDoc, onClose = () => {}) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <LegalModal doc={doc} onClose={onClose} />
    </QueryClientProvider>,
  );
}

describe("LegalModal", () => {
  it("shows the requested document's title and text", async () => {
    vi.mocked(api.getLegalText).mockResolvedValue("GNU AFFERO GENERAL PUBLIC LICENSE…");
    renderModal("license");
    expect(screen.getByText("GNU Affero General Public License v3")).toBeTruthy();
    await waitFor(() =>
      expect(screen.getByText("GNU AFFERO GENERAL PUBLIC LICENSE…")).toBeTruthy(),
    );
    expect(vi.mocked(api.getLegalText)).toHaveBeenCalledWith("license");
  });

  it("surfaces a load error instead of an empty pane", async () => {
    vi.mocked(api.getLegalText).mockRejectedValue("No such resource");
    renderModal("notices");
    await waitFor(() => expect(screen.getByText("No such resource")).toBeTruthy());
  });

  it("closes via the X button", async () => {
    vi.mocked(api.getLegalText).mockResolvedValue("text");
    const onClose = vi.fn();
    renderModal("exception", onClose);
    fireEvent.click(screen.getByLabelText("Close"));
    expect(onClose).toHaveBeenCalled();
  });
});
