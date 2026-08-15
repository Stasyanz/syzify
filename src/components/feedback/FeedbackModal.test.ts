import { describe, it, expect } from "vitest";
import { buildMailtoUrl } from "./FeedbackModal";

describe("buildMailtoUrl", () => {
  it("builds correct mailto URL for bug report", () => {
    const url = buildMailtoUrl("test@example.com", "bug", "Something is broken");

    // Recipient is a known-shape constant, left unencoded — "@" is a literal
    // in mailto: per RFC 6068, and some clients don't decode a "%40".
    expect(url).toContain("mailto:test@example.com");
    // mailto: query is a plain URI (RFC 3986), not
    // application/x-www-form-urlencoded, so spaces must be %20, not "+".
    expect(url).toContain("subject=%5BBug%20Report%5D");
    expect(url).toContain("Feedback%20from%20Syzify");
    expect(url).toContain("Something%20is%20broken");
    expect(url).toContain("App%20version%3A%20");
  });

  it("builds correct mailto URL for feature request", () => {
    const url = buildMailtoUrl("test@example.com", "feature", "Add dark mode");

    expect(url).toContain("subject=%5BFeature%20Request%5D");
    expect(url).toContain("Add%20dark%20mode");
  });

  it("encodes special characters in message", () => {
    const url = buildMailtoUrl(
      "test@example.com",
      "bug",
      "Line 1\nLine 2 & special <chars>",
    );

    // Should not contain raw & or < that would break URL parsing
    expect(url).toContain("Line%201%0ALine%202%20%26%20special%20%3Cchars%3E");
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
