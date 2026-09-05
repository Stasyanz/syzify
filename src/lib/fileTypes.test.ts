import { describe, it, expect } from "vitest";
import { isFolderLikePath, isImportablePath, isImagePath, isWorkoutPath } from "./fileTypes";

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

  it("accepts gzipped workouts, not any .gz", () => {
    expect(isWorkoutPath("/a/ride.fit.gz")).toBe(true);
    expect(isWorkoutPath("/a/run.GPX.GZ")).toBe(true);
    expect(isWorkoutPath("/a/archive.tar.gz")).toBe(false);
    expect(isWorkoutPath("/a/blob.gz")).toBe(false);
    expect(isImportablePath("/a/ride.fit.gz")).toBe(true);
  });

  it("rejects images and other files", () => {
    expect(isWorkoutPath("photo.jpg")).toBe(false);
    expect(isWorkoutPath("archive.zip")).toBe(false);
    expect(isWorkoutPath("")).toBe(false);
  });
});

describe("isFolderLikePath / isImportablePath", () => {
  it("lets an extension-less path through as a possible folder", () => {
    expect(isFolderLikePath("/Users/me/garmin/Monitor")).toBe(true);
    expect(isFolderLikePath("/Users/me/garmin/Monitor/")).toBe(true);
    expect(isFolderLikePath("C:\\Garmin\\Monitor")).toBe(true);
    expect(isFolderLikePath("")).toBe(false);
  });

  it("lets a folder with a dot in its name through too", () => {
    expect(isFolderLikePath("/backups/Garmin 2.0")).toBe(true);
    expect(isFolderLikePath("/backups/2024.backup")).toBe(true);
    expect(isFolderLikePath("/backups/.hidden")).toBe(true);
  });

  it("keeps known file types out so they get the friendly toast", () => {
    expect(isFolderLikePath("/a/ride.fit")).toBe(false);
    expect(isFolderLikePath("/a/notes.txt")).toBe(false);
    expect(isFolderLikePath("/a/photo.JPG")).toBe(false);
    expect(isFolderLikePath("/a/archive.zip")).toBe(false);
  });

  it("isImportablePath accepts workouts and possible folders", () => {
    expect(isImportablePath("/a/Monitor")).toBe(true);
    expect(isImportablePath("/a/ride.FIT")).toBe(true);
    expect(isImportablePath("/a/photo.jpg")).toBe(false);
    expect(isImportablePath("/a/notes.txt")).toBe(false);
  });
});
