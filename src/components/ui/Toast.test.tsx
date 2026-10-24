// @vitest-environment happy-dom
import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ToastContainer } from "./Toast";
import { useToastStore } from "../../stores/toastStore";
import { useLanguageStore } from "../../lib/i18n";

afterEach(() => {
  cleanup();
  useToastStore.setState({ toasts: [] });
  useLanguageStore.getState().setPref("system");
});

describe("ToastContainer", () => {
  it("shows a toast with a dismiss button that removes it", () => {
    render(<ToastContainer />);
    act(() => {
      useToastStore.getState().addToast("success", "Saved", undefined, true);
    });
    expect(screen.getByText("Saved")).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Dismiss"));
    expect(screen.queryByText("Saved")).toBeNull();
  });

  it("names the dismiss button in the interface language (#235)", () => {
    useLanguageStore.getState().setPref("ru");
    render(<ToastContainer />);
    act(() => {
      useToastStore.getState().addToast("info", "Готово", undefined, true);
    });
    expect(screen.getByLabelText("Скрыть")).toBeTruthy();
  });
});
