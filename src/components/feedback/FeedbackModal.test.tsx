// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, fireEvent, waitFor } from "@testing-library/react";
import { FeedbackModal } from "./FeedbackModal";
import { useFeedbackStore } from "../../stores/feedbackStore";
import { useToastStore } from "../../stores/toastStore";
import { CONTACT_EMAIL, GITHUB_ISSUES_URL } from "../../lib/contact";
import { copyText } from "../../lib/clipboard";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../lib/clipboard", () => ({
  copyText: vi.fn(),
}));

import { openUrl } from "@tauri-apps/plugin-opener";

afterEach(() => {
  cleanup();
  useFeedbackStore.getState().close();
  useToastStore.setState({ toasts: [] });
  // Mock instances persist across tests (module-level, not reset here) —
  // drop calls leaked from the previous test rather than resetting modules.
  vi.mocked(openUrl).mockClear();
  vi.mocked(copyText).mockClear();
});

function openModal() {
  useFeedbackStore.getState().open();
  return render(<FeedbackModal />);
}

describe("FeedbackModal", () => {
  it("renders nothing when closed", () => {
    render(<FeedbackModal />);
    expect(screen.queryByText("Send Feedback")).toBeNull();
  });

  it("sends a valid message via mailto and closes", async () => {
    openModal();

    fireEvent.change(screen.getByPlaceholderText("Describe the issue or suggestion..."), {
      target: { value: "This is a long enough message" },
    });
    fireEvent.click(screen.getByText("Send"));

    await waitFor(() => expect(openUrl).toHaveBeenCalledTimes(1));
    const url = vi.mocked(openUrl).mock.calls[0][0];
    expect(url).toContain("mailto:");
    expect(url).toContain("This%20is%20a%20long%20enough%20message");

    await waitFor(() => expect(screen.queryByText("Send Feedback")).toBeNull());
  });

  it("rejects a message shorter than 10 characters and does not open mail client", async () => {
    openModal();

    fireEvent.change(screen.getByPlaceholderText("Describe the issue or suggestion..."), {
      target: { value: "short" },
    });
    fireEvent.click(screen.getByText("Send"));

    expect(await screen.findByText("Message must be at least 10 characters")).toBeTruthy();
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("shows a success toast when the address copy succeeds", async () => {
    vi.mocked(copyText).mockResolvedValue(true);
    openModal();

    fireEvent.click(screen.getByLabelText("Copy address"));

    await waitFor(() =>
      expect(useToastStore.getState().toasts.some((t) => t.message === "Address copied")).toBe(
        true,
      ),
    );
  });

  it("shows an error toast when the address copy fails", async () => {
    vi.mocked(copyText).mockResolvedValue(false);
    openModal();

    fireEvent.click(screen.getByLabelText("Copy address"));

    await waitFor(() =>
      expect(
        useToastStore.getState().toasts.some((t) => t.type === "error" && /select the address/.test(t.message)),
      ).toBe(true),
    );
  });

  it("shows the contact address and a link to the GitHub issue tracker", () => {
    openModal();

    expect(screen.getByText(CONTACT_EMAIL)).toBeTruthy();
    const link = screen.getByText("Open an issue") as HTMLAnchorElement;
    expect(link.getAttribute("href")).toBe(GITHUB_ISSUES_URL);
    expect(link.getAttribute("href")).toMatch(/\/issues$/);
  });
});
