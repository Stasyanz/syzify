// @vitest-environment happy-dom
import { describe, it, expect, afterEach, vi } from "vitest";
import { render, cleanup, screen, waitFor, fireEvent, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ViewSpec } from "../../lib/types";
import { api } from "../../lib/tauri";
import { MAX_CONTINUE_ROUNDS, PluginWidget } from "./PluginWidget";

vi.mock("../../lib/tauri", () => ({
  api: { renderPluginView: vi.fn() },
}));
const mocked = vi.mocked(api);

const status: ViewSpec = {
  title: "Sync demo",
  elements: [
    { type: "text", text: "0 of 3 steps synced." },
    { type: "button", label: "Sync now", action: "sync" },
  ],
};
const round = (n: number, more: boolean): ViewSpec => ({
  title: "Sync demo",
  elements: [{ type: "text", text: `Syncing step ${n} of 3…` }],
  continue: more ? "sync" : null,
});

function renderWidget(followContinue: boolean) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <PluginWidget pluginId="com.test.sync" name="Sync demo" point="sync.source" followContinue={followContinue} />
    </QueryClientProvider>,
  );
}

/** The `action` of the n-th render call's context (the initial render has none). */
function actionOf(call: number): string | undefined {
  return JSON.parse(mocked.renderPluginView.mock.calls[call][2]).action;
}

const tick = () => act(() => new Promise((r) => setTimeout(r, 20)));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("PluginWidget continue loop", () => {
  it("follows continue after a button until a view without it", async () => {
    mocked.renderPluginView
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(round(1, true))
      .mockResolvedValueOnce(round(2, true))
      .mockResolvedValueOnce(round(3, false));
    renderWidget(true);
    fireEvent.click(await screen.findByText("Sync now"));
    await waitFor(() => expect(screen.getByText("Syncing step 3 of 3…")).toBeTruthy());
    await tick();
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(4);
    expect([actionOf(1), actionOf(2), actionOf(3)]).toEqual(["sync", "sync", "sync"]);
    expect(screen.queryByText("Stop")).toBeNull();
  });

  it("stops after the in-flight round when Stop is pressed", async () => {
    // Every round asks for another; only Stop ends it.
    let n = 0;
    mocked.renderPluginView.mockImplementation(async (_id, _point, ctx) =>
      JSON.parse(ctx).action ? round(++n, true) : status,
    );
    renderWidget(true);
    fireEvent.click(await screen.findByText("Sync now"));
    await waitFor(() => expect(mocked.renderPluginView.mock.calls.length).toBeGreaterThanOrEqual(3));
    fireEvent.click(await screen.findByText("Stop"));
    // The stop flag is set in the click handler; no round can start after it.
    const calls = mocked.renderPluginView.mock.calls.length;
    await tick();
    await tick();
    expect(mocked.renderPluginView.mock.calls.length).toBe(calls);
    expect(screen.queryByText("Stop")).toBeNull();
  });

  it("ignores continue on a widget and on the initial render", async () => {
    mocked.renderPluginView
      .mockResolvedValueOnce({ ...status, continue: "sync" })
      .mockResolvedValueOnce(round(1, true));
    renderWidget(false);
    await screen.findByText("Sync now");
    await tick();
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByText("Sync now"));
    await waitFor(() => expect(screen.getByText("Syncing step 1 of 3…")).toBeTruthy());
    await tick();
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(2);
    expect(screen.queryByText("Stop")).toBeNull();
  });

  it("treats an empty continue as none, and inputs are inert while a round runs", async () => {
    const pending: Array<(spec: ViewSpec) => void> = [];
    const form: ViewSpec = {
      title: null,
      elements: [
        { type: "input", id: "email", label: "Email", value: "", input_type: "text" },
        { type: "select", id: "mode", label: "Mode", options: ["all", "new"], value: "all" },
        { type: "button", label: "Sync now", action: "sync" },
      ],
    };
    mocked.renderPluginView.mockImplementation((_id, _point, ctx) =>
      JSON.parse(ctx).action ? new Promise<ViewSpec>((resolve) => pending.push(resolve)) : Promise.resolve(form),
    );
    renderWidget(true);
    fireEvent.click(await screen.findByText("Sync now"));
    await waitFor(() => expect(pending).toHaveLength(1));
    // The first round may take the whole budget: Stop is there already.
    expect(screen.getByText("Stop")).toBeTruthy();
    expect((screen.getByLabelText("Email") as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "Mode" }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => pending[0]({ ...form, continue: "" }));
    await tick();
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(2);
    expect(screen.queryByText("Stop")).toBeNull();
    expect((screen.getByLabelText("Email") as HTMLInputElement).disabled).toBe(false);
  });

  it("an initial render with continue does not start a loop on a page either", async () => {
    mocked.renderPluginView.mockResolvedValueOnce({ ...status, continue: "sync" });
    renderWidget(true);
    await screen.findByText("Sync now");
    await tick();
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(1);
  });

  it("ends the loop on an error and shows it", async () => {
    mocked.renderPluginView
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(round(1, true))
      .mockRejectedValueOnce(new Error("vault busy: a backup is running"));
    renderWidget(true);
    fireEvent.click(await screen.findByText("Sync now"));
    await waitFor(() => expect(screen.getByText(/vault busy/)).toBeTruthy());
    await tick();
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(3);
    expect(screen.queryByText("Stop")).toBeNull();
  });

  it("runs one chain at a time: buttons are inert while a loop runs, Stop drops the round in flight, a new press restarts", async () => {
    // Each round's answer is released by hand.
    const pending: Array<(spec: ViewSpec) => void> = [];
    const withButton = (spec: ViewSpec): ViewSpec => ({ ...spec, elements: [...spec.elements, status.elements[1]] });
    mocked.renderPluginView.mockImplementation((_id, _point, ctx) =>
      JSON.parse(ctx).action ? new Promise<ViewSpec>((resolve) => pending.push(resolve)) : Promise.resolve(status),
    );
    renderWidget(true);
    fireEvent.click(await screen.findByText("Sync now"));
    await waitFor(() => expect(pending).toHaveLength(1));
    expect((screen.getByText("Sync now") as HTMLButtonElement).disabled).toBe(true);

    await act(async () => pending[0](withButton(round(1, true))));
    await waitFor(() => expect(pending).toHaveLength(2));
    expect(screen.getByText("Working… round 1")).toBeTruthy();
    expect((screen.getByText("Sync now") as HTMLButtonElement).disabled).toBe(true);

    // Stop while round 2 is in flight: its answer shows, nothing follows.
    fireEvent.click(screen.getByText("Stop"));
    await act(async () => pending[1](withButton(round(2, true))));
    await tick();
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(3);
    expect(screen.getByText("Syncing step 2 of 3…")).toBeTruthy();
    expect(screen.queryByText("Stop")).toBeNull();
    expect((screen.getByText("Sync now") as HTMLButtonElement).disabled).toBe(false);

    // A new press: a fresh chain, rounds counted from the start.
    fireEvent.click(screen.getByText("Sync now"));
    await waitFor(() => expect(pending).toHaveLength(3));
    await act(async () => pending[2](withButton(round(3, true))));
    await waitFor(() => expect(screen.getByText("Working… round 1")).toBeTruthy());
    expect(mocked.renderPluginView).toHaveBeenCalledTimes(5);
  });

  it("stops at the round cap and says so", async () => {
    let n = 0;
    mocked.renderPluginView.mockImplementation(async (_id, _point, ctx) =>
      JSON.parse(ctx).action ? round(++n, true) : status,
    );
    renderWidget(true);
    fireEvent.click(await screen.findByText("Sync now"));
    await waitFor(
      () => expect(screen.getByText(new RegExp(`Stopped after ${MAX_CONTINUE_ROUNDS} rounds`))).toBeTruthy(),
      { timeout: 10_000 },
    );
    const calls = mocked.renderPluginView.mock.calls.length;
    expect(calls).toBe(1 + MAX_CONTINUE_ROUNDS + 1);
    await tick();
    expect(mocked.renderPluginView.mock.calls.length).toBe(calls);
    expect(screen.queryByText("Stop")).toBeNull();
  }, 15_000);

  it("does nothing more after the page went away", async () => {
    mocked.renderPluginView
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(round(1, true))
      .mockResolvedValueOnce(round(2, true));
    const view = renderWidget(true);
    fireEvent.click(await screen.findByText("Sync now"));
    await waitFor(() => expect(mocked.renderPluginView).toHaveBeenCalledTimes(2));
    view.unmount();
    const calls = mocked.renderPluginView.mock.calls.length;
    await tick();
    await tick();
    expect(mocked.renderPluginView.mock.calls.length).toBe(calls);
  });
});
