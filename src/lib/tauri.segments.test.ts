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

describe("segment api wrappers", () => {
  it("map to their commands with snake_case names and camelCase args", async () => {
    await api.listSegments();
    expect(mocked).toHaveBeenCalledWith("list_segments");

    await api.renameSegment("seg-1", "New name");
    expect(mocked).toHaveBeenCalledWith("rename_segment", { id: "seg-1", name: "New name" });

    await api.deleteSegment("seg-1");
    expect(mocked).toHaveBeenCalledWith("delete_segment", { id: "seg-1" });

    await api.getSegmentEfforts("seg-1");
    expect(mocked).toHaveBeenCalledWith("get_segment_efforts", { id: "seg-1" });

    await api.getActivitySegmentEfforts("act-1");
    expect(mocked).toHaveBeenCalledWith("get_activity_segment_efforts", { activityId: "act-1" });

    await api.checkSimilarSegments("act-1", 10, 90);
    expect(mocked).toHaveBeenCalledWith("check_similar_segments", {
      activityId: "act-1",
      startIdx: 10,
      endIdx: 90,
    });

    await api.saveSegment("act-1", 10, 90, "Hill");
    expect(mocked).toHaveBeenCalledWith("save_segment", {
      activityId: "act-1",
      startIdx: 10,
      endIdx: 90,
      name: "Hill",
    });
  });
});

describe("update api wrapper", () => {
  it("maps to its command", async () => {
    await api.checkForUpdates();
    expect(mocked).toHaveBeenCalledWith("check_for_updates");
  });

  it("maps install to its command", async () => {
    await api.installUpdate();
    expect(mocked).toHaveBeenCalledWith("install_update");
  });
});
