import { describe, it, expect } from "vitest";
import { CONTACT_EMAIL, GITHUB_ISSUES_URL } from "./contact";

describe("contact constants", () => {
  it("CONTACT_EMAIL is a non-empty, well-formed address", () => {
    expect(CONTACT_EMAIL).toMatch(/^[^@\s]+@[^@\s]+\.[^@\s]+$/);
  });

  it("GITHUB_ISSUES_URL points at a GitHub issues page, not the repo root", () => {
    expect(GITHUB_ISSUES_URL).toMatch(/^https:\/\/github\.com\/.+\/issues$/);
  });
});
