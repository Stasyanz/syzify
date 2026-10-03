import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

/** Every source file under src/, tests left out. */
function sources(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) sources(p, out);
    else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) && !name.endsWith(".d.ts")) out.push(p);
  }
  return out;
}

/**
 * A Tauri command rejects with a bare string, so a handler that annotates
 * its parameter as `Error` and reads `.message` says "undefined" (#174).
 * The registered `unknown` error type catches the un-annotated form at
 * compile time; the annotated form it cannot, so this scan does.
 */
describe("error handlers (#174)", () => {
  it("never annotate a rejection or catch parameter as Error", () => {
    const offenders: string[] = [];
    for (const file of sources(join(__dirname, ".."))) {
      const text = readFileSync(file, "utf8");
      for (const re of [/onError:\s*(?:async\s*)?\(\s*\w+\s*:\s*Error\b/g, /\.catch\(\s*(?:async\s*)?\(\s*\w+\s*:\s*Error\b/g]) {
        for (const m of text.matchAll(re)) {
          const line = text.slice(0, m.index).split("\n").length;
          offenders.push(`${file}:${line}: ${m[0]}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  /** `${e}` and `String(e)` say "Error: …" for a thrown Error; errorText
   * says the message alone for both shapes (#182). The names come from
   * the file's own handlers — `catch (x)`, `.catch((x)`, `onError: (x`
   * — plus a query result's `error`, so a handler named `reason` is
   * scanned as well as one named `e`. */
  it("format a caught value through errorText, never `${x}` or String(x)", () => {
    const offenders: string[] = [];
    for (const file of sources(join(__dirname, ".."))) {
      if (file.endsWith("/lib/errors.ts")) continue;
      const text = readFileSync(file, "utf8");
      const names = new Set(["error"]);
      for (const re of [/catch\s*\(\s*(\w+)/g, /\.catch\(\s*(?:async\s*)?\(?\s*(\w+)/g, /onError:\s*(?:async\s*)?\(\s*(\w+)/g]) {
        for (const m of text.matchAll(re)) names.add(m[1]);
      }
      const alt = [...names].join("|");
      for (const re of [new RegExp(`\\$\\{(?:${alt})\\}`, "g"), new RegExp(`\\bString\\((?:${alt})\\)`, "g")]) {
        for (const m of text.matchAll(re)) {
          const line = text.slice(0, m.index).split("\n").length;
          offenders.push(`${file}:${line}: ${m[0]}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});
