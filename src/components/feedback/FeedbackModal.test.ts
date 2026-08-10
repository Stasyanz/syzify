import { describe, it, expect } from "vitest";
import { buildMailtoUrl } from "./FeedbackModal";

describe("buildMailtoUrl", () => {
  it("builds correct mailto URL for bug report", () => {
    const url = buildMailtoUrl("test@example.com", "bug", "Something is broken");

    expect(url).toContain("mailto:test%40example.com");
    expect(url).toContain("subject=%5BBug+Report%5D");
    expect(url).toContain("Feedback+from+Syzify");
    expect(url).toContain("Something+is+broken");
    expect(url).toContain("App+version%3A+");
  });

  it("builds correct mailto URL for feature request", () => {
    const url = buildMailtoUrl("test@example.com", "feature", "Add dark mode");

    expect(url).toContain("subject=%5BFeature+Request%5D");
    expect(url).toContain("Add+dark+mode");
  });

  it("encodes special characters in message", () => {
    const url = buildMailtoUrl(
      "test@example.com",
      "bug",
      "Line 1\nLine 2 & special <chars>",
    );

    // Should not contain raw & or < that would break URL parsing
    expect(url).toContain("Line+1%0ALine+2");
    expect(url).toContain("%26");
    expect(url).toContain("%3C");
    expect(url).toContain("%3E");
  });

  it("includes all required parts", () => {
    const url = buildMailtoUrl("support@app.com", "bug", "Test message here");

    // Starts with mailto:
    expect(url.startsWith("mailto:")).toBe(true);
    // Has subject and body params
    expect(url).toContain("subject=");
    expect(url).toContain("body=");
  });
});
