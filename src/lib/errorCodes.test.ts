import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { en } from "../locales/en";
import type { PluralMessage } from "../locales/types";

/** Every Rust source under src-tauri/src. */
function rustFiles(dir: string, out: { path: string; text: string }[] = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) rustFiles(p, out);
    else if (name.endsWith(".rs")) out.push({ path: p, text: readFileSync(p, "utf8") });
  }
  return out;
}

// ── A small Rust reader: enough to step over strings, chars and comments ──

/** The index after the string literal (plain or raw) starting at `i`. */
function stringEnd(text: string, i: number): number {
  if (text[i] === "r") {
    let hashes = 0;
    let j = i + 1;
    while (text[j] === "#") (hashes++, j++);
    const close = '"' + "#".repeat(hashes);
    const end = text.indexOf(close, j + 1);
    return end < 0 ? text.length : end + close.length;
  }
  let j = i + 1;
  while (j < text.length && text[j] !== '"') j += text[j] === "\\" ? 2 : 1;
  return j + 1;
}

/** The index after what starts at `i` when it is a string, a char or a
 * comment; else `i + 1`. */
function step(text: string, i: number): number {
  const c = text[i];
  if (c === "/" && text[i + 1] === "/") {
    const nl = text.indexOf("\n", i);
    return nl < 0 ? text.length : nl;
  }
  if (c === "/" && text[i + 1] === "*") {
    const end = text.indexOf("*/", i + 2);
    return end < 0 ? text.length : end + 2;
  }
  if (c === '"' || (c === "r" && /^r#*"/.test(text.slice(i, i + 4)) && !/\w/.test(text[i - 1] ?? ""))) {
    return stringEnd(text, i);
  }
  if (c === "'") {
    // '\n', '\'' and '\u{1F600}': an escape, then the closing quote.
    if (text[i + 1] === "\\") {
      const end = text.indexOf("'", i + 3);
      return end < 0 ? text.length : end + 1;
    }
    if (text[i + 2] === "'") return i + 3;
  }
  return i + 1;
}

/** The index of the bracket closing the one at `open`. */
function closing(text: string, open: number): number {
  const pair: Record<string, string> = { "(": ")", "{": "}", "[": "]" };
  const stack = [pair[text[open]]];
  let i = open + 1;
  while (stack.length && i < text.length) {
    const c = text[i];
    if (c in pair) stack.push(pair[c]);
    else if (c === stack[stack.length - 1]) stack.pop();
    if (!stack.length) return i;
    i = step(text, i);
  }
  return i;
}

/** A string literal's text: escapes resolved, a `\`-newline continuation joined. */
function literal(source: string): string {
  if (source.startsWith("r")) return source.replace(/^r#*"/, "").replace(/"#*$/, "");
  return source
    .slice(1, -1)
    .replace(/\\\n\s*/g, "")
    .replace(/\\(.)/g, (_, c: string) => ({ n: "\n", t: "\t" })[c] ?? c);
}

/** The spans of the `#[cfg(test)] mod … { … }` blocks. */
function testSpans(text: string): [number, number][] {
  return [...text.matchAll(/#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{/g)].map((m) => {
    const open = m.index + m[0].length - 1;
    return [m.index, closing(text, open)];
  });
}

interface Site {
  where: string;
  inTest: boolean;
  /** The code as written: a literal's text, or the expression. */
  code: string;
  codeIsLiteral: boolean;
  /** The message: a literal's text, a `format!`'s template, or null when it
   * is an expression (a variable, a call). */
  message: string | null;
  isFormat: boolean;
  params: string[];
}

/** Every call of `coded(` outside a comment, with its code, its message
 * and the `.param("…")`s chained on it. */
function codedSites(): Site[] {
  const sites: Site[] = [];
  for (const { path, text } of rustFiles(join(__dirname, "../../src-tauri/src"))) {
    const spans = testSpans(text);
    let i = 0;
    while (i < text.length) {
      const m = /^\bcoded\(/.exec(text.slice(i, i + 6));
      const isCall = m && !/\w/.test(text[i - 1] ?? "") && !/\bfn\s+$/.test(text.slice(Math.max(0, i - 8), i));
      if (!isCall) {
        i = step(text, i);
        continue;
      }
      const open = i + 5;
      const close = closing(text, open);
      const args = text.slice(open + 1, close);
      const codeArg = /^\s*("(?:[^"\\]|\\.)*")\s*,/.exec(args);
      const rest = codeArg ? args.slice(codeArg[0].length) : "";
      const msgLit = /^\s*("(?:[^"\\]|\\.)*")\s*,?\s*$/.exec(rest);
      const fmt = /^\s*format!\(\s*("(?:[^"\\]|\\.)*")/.exec(rest);
      const params: string[] = [];
      let j = close + 1;
      for (;;) {
        const p = /^\s*\.param\(\s*"(\w+)"/.exec(text.slice(j));
        if (!p) break;
        params.push(p[1]);
        j = closing(text, text.indexOf("(", j)) + 1;
      }
      sites.push({
        where: `${path.replace(/^.*src-tauri\//, "")}:${text.slice(0, i).split("\n").length}`,
        inTest: spans.some(([a, b]) => i > a && i < b),
        code: codeArg ? literal(codeArg[1]) : args.split(",")[0].trim(),
        codeIsLiteral: !!codeArg,
        message: msgLit ? literal(msgLit[1]) : fmt ? literal(fmt[1]) : null,
        isFormat: !msgLit && !!fmt,
        params,
      });
      i = close + 1;
    }
  }
  return sites;
}

const SITES = codedSites();
const ENGLISH = en as Record<string, string | PluralMessage>;

/** The English of a code: a plural message's `other` form, which every
 * limit the app sets picks. */
function english(code: string): string | undefined {
  const m = ENGLISH[`error.${code}`];
  return m === undefined ? undefined : typeof m === "string" ? m : m.other;
}

const escape = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]);

/** Whether a message as Rust writes it says what English says. A plain
 * literal in the code is the English exactly; a `format!` template has a
 * hole (`{}`, `{name}`) exactly where English has a placeholder, and the
 * same text around; a test's literal is a filled message, each placeholder
 * standing for whatever was filled in. */
function saysTheEnglish(site: Site, text: string): boolean {
  const message = english(site.code)!;
  if (site.inTest) return new RegExp(`^${escape(message).replace(/\\\{\w+\\\}/g, ".+")}$`).test(text);
  if (!site.isFormat) return text === message;
  const holes = text.replace(/\{\{|\}\}/g, "").match(/\{[^{}]*\}/g) ?? [];
  const re = new RegExp(`^${escape(message).replace(/\\\{\w+\\\}/g, "\\{[^{}]*\\}")}$`);
  return re.test(text) && holes.length === placeholders(message).length;
}

/**
 * The codes Rust refuses with (#236) and the `error.*` messages are one
 * list, and say the same: a code without a message would say English in
 * every language, a message without a code is dead, and an English message
 * that drifted from Rust's would read differently from the fallback.
 */
describe("backend error codes", () => {
  it("are literals at every call", () => {
    expect(SITES.length).toBeGreaterThan(50);
    expect(SITES.filter((s) => !s.codeIsLiteral || !/^[\w.]+$/.test(s.code)).map((s) => s.where)).toEqual([]);
  });

  it("each have an error.* message, and no message is without its code", () => {
    const codes = [...new Set(SITES.map((s) => s.code))].map((c) => `error.${c}`).sort();
    const keys = Object.keys(en).filter((k) => k.startsWith("error."));
    expect(codes).toEqual(keys.sort());
  });

  it("say in English what Rust says", () => {
    const drift = SITES.filter((s) => s.message !== null && english(s.code) !== undefined)
      .filter((s) => !saysTheEnglish(s, s.message!))
      .map((s) => `${s.where} ${s.code}: ${s.message}`);
    expect(drift).toEqual([]);
  });

  it("each have their English checked at least once", () => {
    const checked = new Set(SITES.filter((s) => s.message !== null).map((s) => s.code));
    expect([...new Set(SITES.map((s) => s.code))].filter((c) => !checked.has(c))).toEqual([]);
  });

  it("send the params their English fills, by name", () => {
    const wrong = SITES.filter((s) => !s.inTest && english(s.code) !== undefined)
      .filter((s) => {
        const message = ENGLISH[`error.${s.code}`];
        const forms = typeof message === "string" ? [message] : Object.values(message);
        const wanted = [...new Set(forms.flatMap(placeholders))].sort();
        return wanted.join() !== [...s.params].sort().join();
      })
      .map((s) => `${s.where} ${s.code}: ${s.params.join(",")}`);
    expect(wrong).toEqual([]);
  });

  it("are read whole by the scanner", () => {
    const sample = [
      '#[cfg(test)]\nmod tests {\n  fn t() { let _ = CommandError::coded("x.y", "a {b}").param("b", f(1)); }\n}',
      'fn e() -> CommandError { coded(\n  "x.z",\n  format!("{} \\"{q}\\" (\'{\')", 1),\n)\n.param("q", q) }',
    ].join("\n");
    const spans = testSpans(sample);
    expect(spans).toHaveLength(1);
    expect(sample.slice(spans[0][0], spans[0][1] + 1).endsWith("}\n}")).toBe(true);
    expect(literal('"a \\"b\\" \\\n    c"')).toBe('a "b" c');
    expect(literal('r#"x "y" z"#')).toBe('x "y" z');
    const open = sample.indexOf("coded(\n") + 5;
    expect(sample.slice(closing(sample, open) + 1)).toMatch(/^\n\.param\("q"/);
  });
});
