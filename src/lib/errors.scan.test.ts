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
 * A rejection may be a bare string, so a handler that annotates its
 * parameter as `Error` and reads `.message` says "undefined" (#174) — or
 * a command's coded error, whose translation `.message` skips (#236).
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

  /** A caught value said in an expression the handler-name scan above
   * cannot follow — `String(save.isError ? save.error : similarError)`
   * said "[object Object]" for a CommandError (#236). */
  it("never stringify anything named like an error, wherever it comes from", () => {
    const offenders = sources(join(__dirname, ".."))
      .filter((file) => !file.endsWith("/lib/errors.ts"))
      .flatMap((file) => stringifiedErrors(readFileSync(file, "utf8")).map((m) => `${file}: ${m}`));
    expect(offenders).toEqual([]);
  });

  it("catch every shape of a stringified error, and nothing else", () => {
    const caught = [
      "{String(save.isError ? save.error : similarError)}",
      "renameError={String(rename.error)}",
      "toast(String(e))",
      "toast(String( err ))",
      "msg = `Failed: ${e}`",
      "msg = `Failed: ${err}`",
      "msg = `Failed: ${error}`",
      "msg = `Failed: ${query.error}`",
      "msg = `Failed: ${saveError}`",
    ];
    for (const line of caught) expect(stringifiedErrors(line), line).toHaveLength(1);
    const fine = [
      "errorText(save.error)",
      "String(date)",
      "String(value)",
      "`${errorText(e)}`",
      "`${errorCount} errors`",
      "`${name}`",
    ];
    for (const line of fine) expect(stringifiedErrors(line), line).toEqual([]);
  });
});

/** `String(…x)` and `${…x}` where x is `e`, or a name ending in "err" or
 * "error" (`err`, `error`, `rename.error`, `similarError`). */
function stringifiedErrors(text: string): string[] {
  const name = String.raw`\b(?:\w*(?:[eE]rror|[eE]rr)|e)`;
  const patterns = [new RegExp(String.raw`\bString\([^()]*?${name}\s*\)`, "g"), new RegExp(String.raw`\$\{\s*[\w.?]*?${name}\s*\}`, "g")];
  return patterns.flatMap((re) => [...text.matchAll(re)].map((m) => m[0]));
}
