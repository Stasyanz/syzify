#!/usr/bin/env node
// Generates THIRD-PARTY-NOTICES.md at the repo root: every third-party
// component shipped in the distributed app (Rust runtime deps, npm production
// deps, bundled fonts), grouped by license, with the license texts appended.
//
// Zero extra dependencies by design: reads `cargo metadata` and
// `npx license-checker` output, and pulls copyright lines / license texts
// from the dependencies' own license files.
//
// Run on release (before `tauri build`):  npm run gen:notices

import { execSync } from "node:child_process";
import { readFileSync, readdirSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";

const ROOT = join(dirname(new URL(import.meta.url).pathname), "..");

// Where a package offers a choice of licenses ("MIT OR Apache-2.0"), we elect
// the first match from this list; the file header states the election rule.
const LICENSE_PREFERENCE = [
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-3-Clause",
  "BSD-2-Clause",
  "ISC",
  "Zlib",
  "BSL-1.0",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "CC0-1.0",
  "Unlicense",
  "0BSD",
  "BSD-1-Clause",
  "MPL-2.0",
  "CDLA-Permissive-2.0",
  "OFL-1.1",
];

function electLicense(expr) {
  if (!expr) return "UNKNOWN";
  // "MPL-2.0+" = "or any later version": we use it under the named version.
  const e = expr.trim().replace(/\+$/, "");
  // Conjunctive expressions ("A AND B") are not a choice — every part
  // applies, so the whole expression stays as the group.
  if (/\bAND\b/.test(e)) return e;
  // Old crates spell dual licensing with a slash: "MIT/Apache-2.0".
  const cleaned = e.replace(/[()]/g, " ").replace(/\s*\/\s*/g, " OR ");
  if (!/\bOR\b/i.test(cleaned)) return e;
  const options = cleaned.split(/\bOR\b/i).map((s) => s.trim());
  for (const pref of LICENSE_PREFERENCE) {
    const hit = options.find((o) => o === pref);
    if (hit) return hit;
  }
  return options[0];
}

function findLicenseFiles(dir) {
  let entries = [];
  try {
    entries = readdirSync(dir);
  } catch {
    return [];
  }
  return entries
    .filter((f) => /^(LICEN[CS]E|COPYING|NOTICE)/i.test(f))
    .map((f) => join(dir, f));
}

function copyrightLines(files) {
  const lines = new Set();
  for (const f of files) {
    let text = "";
    try {
      text = readFileSync(f, "utf8");
    } catch {
      continue;
    }
    for (const line of text.split("\n")) {
      const t = line.replace(/^[/*#\s-]+/, "").trim();
      if (/^copyright\s+(\(c\)|©|\d{4})/i.test(t) && t.length < 200) {
        lines.add(t);
        if (lines.size >= 3) return [...lines];
      }
    }
  }
  return [...lines];
}

// ── Rust: runtime dependency graph of the syzify binary ──────────────────────
const meta = JSON.parse(
  execSync("cargo metadata --format-version 1 --manifest-path src-tauri/Cargo.toml", {
    cwd: ROOT,
    maxBuffer: 256 * 1024 * 1024,
  }),
);
const byId = new Map(meta.packages.map((p) => [p.id, p]));
const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
const rootId = meta.resolve.root;

// BFS over non-dev edges: what actually ships (normal deps, incl. proc-macros).
const shipped = new Set();
const queue = [rootId];
while (queue.length) {
  const id = queue.pop();
  if (shipped.has(id)) continue;
  shipped.add(id);
  for (const dep of nodes.get(id)?.deps ?? []) {
    if (dep.dep_kinds.some((k) => k.kind === null || k.kind === "normal")) queue.push(dep.pkg);
  }
}
shipped.delete(rootId);

const components = [];
for (const id of shipped) {
  const p = byId.get(id);
  if (!p) continue;
  const licFiles = findLicenseFiles(dirname(p.manifest_path));
  components.push({
    eco: "crates.io",
    name: `${p.name} ${p.version}`,
    expr: p.license ?? (p.license_file ? "SEE-LICENSE-FILE" : "UNKNOWN"),
    license: electLicense(p.license),
    copyrights: copyrightLines(licFiles),
    authors: p.authors ?? [],
    licFiles,
  });
}

// ── npm: production dependencies of the frontend bundle ──────────────────────
const lc = JSON.parse(
  execSync("npx --yes license-checker --production --json", { cwd: ROOT, maxBuffer: 64 * 1024 * 1024 }),
);
for (const [pkg, info] of Object.entries(lc)) {
  if (pkg.startsWith("syzify@")) continue;
  const licFiles = info.licenseFile && !/readme/i.test(info.licenseFile) ? [info.licenseFile] : [];
  components.push({
    eco: "npm",
    name: pkg.replace(/@(\d)/, " $1"),
    expr: String(info.licenses),
    license: electLicense(String(info.licenses)),
    copyrights: copyrightLines(licFiles),
    authors: info.publisher ? [info.publisher] : [],
    licFiles,
  });
}

// ── Bundled assets ───────────────────────────────────────────────────────────
components.push({
  eco: "font",
  name: "Archivo (variable font)",
  expr: "OFL-1.1",
  license: "OFL-1.1",
  copyrights: ["Copyright 2019 The Archivo Project Authors (https://github.com/Omnibus-Type/Archivo)"],
  authors: [],
  licFiles: [join(ROOT, "public/fonts/OFL.txt")],
});

// ── Compose the notices file ─────────────────────────────────────────────────
const groups = new Map();
for (const c of components) {
  if (!groups.has(c.license)) groups.set(c.license, []);
  groups.get(c.license).push(c);
}
const sortedLicenses = [...groups.keys()].sort();

// One representative full text per license, taken from a dependency that
// carries it (canonical bodies are identical across projects).
function licenseText(license, comps) {
  const patterns = {
    "Apache-2.0": /apache/i,
    "Apache-2.0 WITH LLVM-exception": /apache/i,
    MIT: /mit/i,
    "MPL-2.0": /mpl|mozilla/i,
    "OFL-1.1": /ofl/i,
  };
  const pat = patterns[license];
  for (const c of comps) {
    for (const f of c.licFiles) {
      if (pat && !pat.test(f) && c.licFiles.length > 1) continue;
      try {
        const text = readFileSync(f, "utf8").trim();
        if (text.length > 100) return { text, source: c.name };
      } catch {
        /* try next */
      }
    }
  }
  return null;
}

let out = `# Third-Party Notices

Syzify bundles the third-party components listed below. Each component is the
copyright of its authors and is used under the license shown; full license
texts are reproduced in the appendix. Where a component offers a choice of
licenses, Syzify uses it under the license listed here.

This file is generated by \`scripts/gen_third_party_notices.mjs\` — do not edit
by hand.
`;

for (const license of sortedLicenses) {
  const comps = groups.get(license).sort((a, b) => a.name.localeCompare(b.name));
  out += `\n## ${license} (${comps.length})\n\n`;
  for (const c of comps) {
    const who = c.copyrights.length
      ? c.copyrights.join("; ")
      : c.authors.length
        ? `Copyright the ${c.name.split(" ")[0]} authors (${c.authors.join(", ")})`
        : `Copyright the ${c.name.split(" ")[0]} authors`;
    const exprNote = c.expr !== license ? ` — licensed \`${c.expr}\`` : "";
    out += `- **${c.name}** (${c.eco})${exprNote} — ${who}\n`;
  }
}

out += `\n# Appendix: license texts\n`;
for (const license of sortedLicenses) {
  const t = licenseText(license, groups.get(license));
  out += `\n## ${license}\n\n`;
  if (t) {
    out += `_Text as included with ${t.source}; the same terms apply to every component listed under this license (with the respective copyright holders)._\n\n`;
    out += "```\n" + t.text.replace(/```/g, "``​`") + "\n```\n";
  } else {
    out += `_See https://spdx.org/licenses/${encodeURIComponent(license)}.html_\n`;
  }
}

writeFileSync(join(ROOT, "THIRD-PARTY-NOTICES.md"), out);
const unknown = components.filter((c) => c.license === "UNKNOWN");
console.log(
  `THIRD-PARTY-NOTICES.md: ${components.length} components, ${sortedLicenses.length} licenses` +
    (unknown.length ? `; UNKNOWN: ${unknown.map((c) => c.name).join(", ")}` : ""),
);