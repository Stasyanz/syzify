// @vitest-environment happy-dom
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, cleanup, fireEvent, waitFor, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../../lib/tauri", () => ({
  api: { updateActivity: vi.fn() },
}));
const addToast = vi.fn();
vi.mock("../../stores/toastStore", () => ({
  useToastStore: (sel: (s: { addToast: typeof addToast }) => unknown) => sel({ addToast }),
}));

import { api } from "../../lib/tauri";
import { SportBadge } from "./SportBadge";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function mount(onChanged = vi.fn()) {
  const qc = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <SportBadge activityId="act-1" sportType="ride" onChanged={onChanged} />
    </QueryClientProvider>,
  );
  return onChanged;
}

describe("SportBadge", () => {
  it("changes the sport from the menu and says so", async () => {
    vi.mocked(api.updateActivity).mockResolvedValue(undefined as never);
    const onChanged = mount();
    fireEvent.click(screen.getByTitle("Ride — change sport"));
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(api.updateActivity).toHaveBeenCalledWith("act-1", { sport_type: "run" }));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("success", "Sport type updated"));
    expect(onChanged).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
  });

  it("a mousedown outside closes the menu; inside it does not", () => {
    mount();
    fireEvent.click(screen.getByTitle("Ride — change sport"));
    fireEvent.mouseDown(screen.getByRole("button", { name: "Run" }));
    expect(screen.queryByRole("button", { name: "Run" })).not.toBeNull();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
  });

  it("picking the current sport only closes the menu", () => {
    mount();
    fireEvent.click(screen.getByTitle("Ride — change sport"));
    fireEvent.click(screen.getByRole("button", { name: "Ride" }));
    expect(api.updateActivity).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Run" })).toBeNull();
  });

  /// A Tauri command refuses with a bare string, not an Error: the toast
  /// must carry its words, not "undefined" (#174).
  it("says a refusal in the backend's words", async () => {
    vi.mocked(api.updateActivity).mockRejectedValueOnce("The vault is locked");
    const onChanged = mount();
    fireEvent.click(screen.getByTitle("Ride — change sport"));
    fireEvent.click(screen.getByRole("button", { name: "Run" }));
    await waitFor(() => expect(addToast).toHaveBeenCalledWith("error", "The vault is locked"));
    expect(onChanged).not.toHaveBeenCalled();
  });
});
