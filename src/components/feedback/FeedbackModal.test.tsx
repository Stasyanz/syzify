// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, fireEvent, waitFor } from "@testing-library/react";
import { FeedbackModal } from "./FeedbackModal";
import { useFeedbackStore } from "../../stores/feedbackStore";
import { useToastStore } from "../../stores/toastStore";
import { CONTACT_EMAIL, GITHUB_ISSUES_URL } from "../../lib/contact";
import { copyText } from "../../lib/clipboard";
import { useLanguageStore } from "../../lib/i18n";

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

    fireEvent.change(screen.getByPlaceholderText("Describe the issue or suggestion…"), {
      target: { value: "This is a long enough message" },
    });
    fireEvent.click(screen.getByText("Send"));

    await waitFor(() => expect(openUrl).toHaveBeenCalledTimes(1));
    const url = vi.mocked(openUrl).mock.calls[0][0];
    expect(url).toContain("mailto:");
    expect(url).toContain("This%20is%20a%20long%20enough%20message");

    await waitFor(() => expect(screen.queryByText("Send Feedback")).toBeNull());
  });

  /** The opener's refusal is a bare string from Tauri, or an Error from
   * the plugin's JS side: the toast says the words alone (#182). */
  for (const [kind, rejection] of [
    ["a string", "no handler for mailto"],
    ["an Error", new Error("no handler for mailto")],
  ] as const) {
    it(`says why the email client did not open with ${kind}`, async () => {
      vi.mocked(openUrl).mockRejectedValueOnce(rejection);
      openModal();
      fireEvent.change(screen.getByPlaceholderText("Describe the issue or suggestion…"), {
        target: { value: "This is a long enough message" },
      });
      fireEvent.click(screen.getByText("Send"));
      await waitFor(() =>
        expect(useToastStore.getState().toasts.slice(-1)[0]).toMatchObject({
          type: "error",
          message: "Failed to open email client: no handler for mailto",
        }),
      );
      // The modal stays open with the text, so the user can copy it.
      expect(screen.getByText("Send Feedback")).toBeTruthy();
    });
  }

  it("rejects a message shorter than 10 characters and does not open mail client", async () => {
    openModal();

    fireEvent.change(screen.getByPlaceholderText("Describe the issue or suggestion…"), {
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

  describe("in Russian (#235)", () => {
    afterEach(() => useLanguageStore.getState().setPref("system"));

    it("speaks the interface language, while the email keeps its English subject", async () => {
      useLanguageStore.getState().setPref("ru");
      openModal();
      expect(screen.getByText("Отправить отзыв")).toBeTruthy();
      expect(screen.getByText("Сообщение об ошибке")).toBeTruthy();
      // The address and the link sit inside their sentences.
      expect(screen.getByText(CONTACT_EMAIL).parentElement!.textContent).toContain("Нет почтового клиента? Напишите на");
      expect(screen.getByText("Создайте issue").parentElement!.textContent).toBe("Удобнее GitHub? Создайте issue");
      const box = screen.getByPlaceholderText("Опишите проблему или предложение…");
      fireEvent.change(box, { target: { value: "short" } });
      fireEvent.click(screen.getByText("Отправить"));
      expect(await screen.findByText("Сообщение слишком короткое: минимум 10 символов")).toBeTruthy();
      fireEvent.change(box, { target: { value: "x".repeat(1501) } });
      expect(screen.getByRole("status").textContent).toBe("Максимум 1500 символов");
      fireEvent.change(box, { target: { value: "Достаточно длинное сообщение" } });
      fireEvent.click(screen.getByText("Отправить"));
      await waitFor(() => expect(openUrl).toHaveBeenCalledTimes(1));
      expect(vi.mocked(openUrl).mock.calls[0][0]).toContain(encodeURIComponent("[Bug Report] Feedback from Syzify"));
      expect(useToastStore.getState().toasts.slice(-1)[0].message).toBe("Почтовый клиент открыт — отправьте письмо");
    });

    it("says the copy outcomes and a refused email client in the interface language", async () => {
      useLanguageStore.getState().setPref("ru");
      vi.mocked(copyText).mockResolvedValueOnce(true).mockResolvedValueOnce(false);
      vi.mocked(openUrl).mockRejectedValueOnce("no handler for mailto");
      openModal();
      fireEvent.click(screen.getByLabelText("Скопировать адрес"));
      await waitFor(() => expect(useToastStore.getState().toasts.map((t) => t.message)).toContain("Адрес скопирован"));
      fireEvent.click(screen.getByLabelText("Скопировать адрес"));
      await waitFor(() =>
        expect(useToastStore.getState().toasts.map((t) => t.message)).toContain(
          "Не удалось скопировать — выделите адрес вручную",
        ),
      );
      fireEvent.change(screen.getByPlaceholderText("Опишите проблему или предложение…"), {
        target: { value: "Достаточно длинное сообщение" },
      });
      fireEvent.click(screen.getByText("Отправить"));
      await waitFor(() =>
        expect(useToastStore.getState().toasts.map((t) => t.message)).toContain(
          "Не удалось открыть почтовый клиент: no handler for mailto",
        ),
      );
      fireEvent.click(screen.getByLabelText("Закрыть"));
      expect(screen.queryByText("Отправить отзыв")).toBeNull();
    });
  });
});
