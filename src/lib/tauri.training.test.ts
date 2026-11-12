import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { api } from "./tauri";

// The wrappers' whole job is the command-name + arg-case mapping — a typo'd
// snake_case name or a camelCase arg key fails only at runtime, so pin them.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const mocked = vi.mocked(invoke);

beforeEach(() => {
  vi.clearAllMocks();
  mocked.mockResolvedValue(undefined);
});

describe("training api wrappers (#273)", () => {
  it("map to their commands with snake_case names and camelCase args", async () => {
    await api.trainingScan();
    expect(mocked).toHaveBeenLastCalledWith("training_scan", { seconds: null });
    await api.trainingScan(8);
    expect(mocked).toHaveBeenLastCalledWith("training_scan", { seconds: 8 });
    await api.trainingConnect("T", ["PL", "PR"], null);
    expect(mocked).toHaveBeenLastCalledWith("training_connect", { trainer: "T", pedals: ["PL", "PR"], strap: null });
    await api.trainingSetTarget(150);
    expect(mocked).toHaveBeenLastCalledWith("training_set_target", { watts: 150 });
    await api.trainingSetTarget(null);
    expect(mocked).toHaveBeenLastCalledWith("training_set_target", { watts: null });
    await api.trainingFinishPending(1_794_420_000);
    expect(mocked).toHaveBeenLastCalledWith("training_finish_pending", { startUnix: 1_794_420_000 });
    await api.trainingDiscardPending(1_794_420_000);
    expect(mocked).toHaveBeenLastCalledWith("training_discard_pending", { startUnix: 1_794_420_000 });

    const bare = [
      [api.trainingBegin, "training_begin"],
      [api.trainingLap, "training_lap"],
      [api.trainingPause, "training_pause"],
      [api.trainingResume, "training_resume"],
      [api.trainingFinish, "training_finish"],
      [api.trainingDiscard, "training_discard"],
      [api.trainingLeave, "training_leave"],
      [api.trainingLive, "training_live"],
      [api.trainingFtp, "training_ftp"],
      [api.trainingPending, "training_pending"],
    ] as const;
    for (const [call, command] of bare) {
      await call();
      expect(mocked).toHaveBeenLastCalledWith(command);
    }
  });
});
