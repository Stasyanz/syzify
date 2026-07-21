import { describe, it, expect } from "vitest";
import { isImagePath, isWorkoutPath } from "./fileTypes";

describe("isImagePath", () => {
  it("accepts supported image extensions (case-insensitive)", () => {
    expect(isImagePath("/a/b/photo.jpg")).toBe(true);
    expect(isImagePath("photo.JPEG")).toBe(true);
    expect(isImagePath("C:\\Users\\me\\pic.PNG")).toBe(true);
    expect(isImagePath("shot.webp")).toBe(true);
  });

  it("rejects non-images and workout files", () => {
    expect(isImagePath("ride.fit")).toBe(false);
    expect(isImagePath("run.gpx")).toBe(false);
    expect(isImagePath("notes.txt")).toBe(false);
    expect(isImagePath("noextension")).toBe(false);
    expect(isImagePath("")).toBe(false);
  });
});

describe("isWorkoutPath", () => {
  it("accepts gpx/fit/tcx (case-insensitive)", () => {
    expect(isWorkoutPath("/x/run.gpx")).toBe(true);
    expect(isWorkoutPath("ride.FIT")).toBe(true);
    expect(isWorkoutPath("swim.tcx")).toBe(true);
  });

  it("rejects images and other files", () => {
    expect(isWorkoutPath("photo.jpg")).toBe(false);
    expect(isWorkoutPath("archive.zip")).toBe(false);
    expect(isWorkoutPath("")).toBe(false);
  });
});
