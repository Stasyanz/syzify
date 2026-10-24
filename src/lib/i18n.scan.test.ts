import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import ts from "typescript";

const SRC = join(__dirname, "..");

/** Every source file under src/ but tests, declarations and the
 * dictionaries themselves. */
function sources(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (p !== join(SRC, "locales")) sources(p, out);
    } else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) && !name.endsWith(".d.ts")) {
      out.push(p);
    }
  }
  return out;
}

/** JSX attributes and object properties whose string reaches the screen. */
const SHOWN_ATTRIBUTES = new Set([
  "aria-label", "ariaLabel", "aria-description", "aria-valuetext", "title", "placeholder", "data-tip", "alt", "label",
]);
const SHOWN_PROPERTIES = new Set([
  "label", "title", "message", "confirmLabel", "cancelLabel", "placeholder", "clearLabel", "ariaLabel", "description", "text", "name",
]);
/** Calls whose argument at the index is shown as it is: `sportLabel`'s
 * fallback is the name of a sport the app does not know. */
const SHOWN_ARGUMENTS: Record<string, number> = { addToast: 1, confirm: 0, alert: 0, sportLabel: 1 };

/** Text that reads the same in every language: names, formats, the
 * training abbreviations the epic keeps (#237). Grow it with care — an
 * English word here is a word nobody translates. */
const SAME_IN_EVERY_LANGUAGE = new Set([
  "Syzify", "GitHub", "OpenStreetMap", "Garmin", "Strava", "Zwift",
  "GPX", "FIT", "TCX", "ZIP", "NP", "TSS", "FTP", "IF", "VI", "W",
]);

/** Whether a text has a word some language writes differently. */
const hasWords = (s: string) =>
  s.split(/[^\p{L}\p{N}]+/u).some((word) => /\p{L}/u.test(word) && !SAME_IN_EVERY_LANGUAGE.has(word));

/** The literal texts an expression can evaluate to: both branches of a
 * `?:`, the fallback of `&&`, `||` and `??`, each side of a `+`. Not the
 * operands of a comparison or a call's arguments — those are not shown. */
function literals(node: ts.Node | undefined): string[] {
  if (!node) return [];
  if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) return [node.text];
  if (ts.isTemplateExpression(node)) return [[node.head.text, ...node.templateSpans.map((s) => s.literal.text)].join(" ")];
  if (ts.isJsxExpression(node)) return literals(node.expression);
  if (
    ts.isParenthesizedExpression(node) ||
    ts.isAsExpression(node) ||
    ts.isSatisfiesExpression(node) ||
    ts.isNonNullExpression(node)
  ) {
    return literals(node.expression);
  }
  if (ts.isConditionalExpression(node)) return [...literals(node.whenTrue), ...literals(node.whenFalse)];
  if (ts.isBinaryExpression(node)) {
    switch (node.operatorToken.kind) {
      case ts.SyntaxKind.AmpersandAmpersandToken:
      case ts.SyntaxKind.BarBarToken:
      case ts.SyntaxKind.QuestionQuestionToken:
        return literals(node.right);
      case ts.SyntaxKind.PlusToken:
        return [...literals(node.left), ...literals(node.right)];
    }
  }
  return [];
}

/** Whether a JSX element says `translate="no"` — HTML's own mark for text
 * that stays as it is in every language (the wordmark). */
function untranslatable(node: ts.Node, sf: ts.SourceFile): boolean {
  const opening = ts.isJsxElement(node) ? node.openingElement : ts.isJsxSelfClosingElement(node) ? node : null;
  return !!opening?.attributes.properties.some(
    (a) => ts.isJsxAttribute(a) && a.name.getText(sf) === "translate" && literals(a.initializer).join() === "no",
  );
}

/** Every English string a source shows, as `line: text`. Skipped: an
 * element marked `translate="no"`, and a line marked `i18n-ignore` — for
 * text that is not shown (a test id, a file name) or is a name (a device
 * model), never for words. */
export function untranslatedIn(fileName: string, text: string): string[] {
  const lines = text.split("\n");
  const sf = ts.createSourceFile(
    fileName,
    text,
    ts.ScriptTarget.Latest,
    true,
    fileName.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );
  const found: string[] = [];
  const report = (node: ts.Node, texts: string[]) => {
    const line = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line;
    if (lines[line].includes("i18n-ignore")) return;
    for (const s of texts.filter(hasWords)) found.push(`${line + 1}: ${s.replace(/\s+/g, " ").trim()}`);
  };
  const visit = (node: ts.Node) => {
    if (untranslatable(node, sf)) return;
    if (ts.isJsxText(node)) {
      report(node, [node.text]);
    } else if (ts.isJsxExpression(node) && (ts.isJsxElement(node.parent) || ts.isJsxFragment(node.parent))) {
      report(node, literals(node.expression));
    } else if (ts.isJsxAttribute(node) && SHOWN_ATTRIBUTES.has(node.name.getText(sf))) {
      report(node, literals(node.initializer));
    } else if (ts.isPropertyAssignment(node) && SHOWN_PROPERTIES.has(node.name.getText(sf))) {
      report(node, literals(node.initializer));
    } else if (ts.isCallExpression(node)) {
      const callee = node.expression.getText(sf).split(".").pop() ?? "";
      if (callee in SHOWN_ARGUMENTS) report(node, literals(node.arguments[SHOWN_ARGUMENTS[callee]]));
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return found;
}

/**
 * How many English strings each file not yet translated still shows. Each
 * stage of #237 moves screens out: a count must match exactly, so a file
 * half done is held to what is left (a new English string in its
 * translated half fails), and a drop is written down here. The epic ends
 * with an empty map. A file not listed must show no English of its own.
 */
const NOT_YET_TRANSLATED: Record<string, number> = {
  "components/activity/ChartPanel.tsx": 4,
  "components/activity/CropOverlay.tsx": 1,
  "components/activity/CyclingDynamicsPanel.tsx": 18,
  "components/activity/DraggableShareBlocks.tsx": 2,
  "components/activity/EditActivityModal.tsx": 35,
  "components/activity/FtpMismatchHint.tsx": 4,
  "components/activity/GearChip.tsx": 9,
  "components/activity/InlineTitle.tsx": 4,
  "components/activity/LapsTable.tsx": 11,
  "components/activity/MultisportLegs.tsx": 9,
  "components/activity/PhotoGallery.tsx": 16,
  "components/activity/PowerCurvePanel.tsx": 5,
  "components/activity/RouteMap.tsx": 16,
  "components/activity/SaveSegmentPopover.tsx": 8,
  "components/activity/SegmentEffortsPanel.tsx": 10,
  "components/activity/ShareModal.tsx": 44,
  "components/activity/SportBadge.tsx": 2,
  "components/activity/SummaryPanel.tsx": 6,
  "components/activity/TimeInZonesPanel.tsx": 3,
  "components/activity/chartConfigs.ts": 7,
  "components/activity/shareCanvas.ts": 2,
  "components/activity/shareLayout.ts": 1,
  "components/activity/timeInZones.ts": 1,
  "components/feedback/FeedbackModal.tsx": 21,
  "components/import/ImportDialog.tsx": 6,
  "components/import/ImportProgressOverlay.tsx": 1,
  "components/plugins/PluginWidget.tsx": 8,
  "components/settings/Garage.tsx": 87,
  "components/settings/LegalModal.tsx": 1,
  "components/settings/MonitoringData.tsx": 10,
  "components/settings/PluginRegistry.tsx": 33,
  "components/settings/UpdateCheck.tsx": 6,
  "components/settings/VaultLocation.tsx": 18,
  "components/settings/WatchFolders.tsx": 30,
  "components/ui/ConfirmDialog.tsx": 2,
  "components/ui/DateField.tsx": 5,
  "components/ui/Toast.tsx": 1,
  "hooks/useDropImport.ts": 6,
  "hooks/useWatchFolderListener.ts": 1,
  "lib/importSummary.ts": 2,
  "lib/settingsTabs.ts": 5,
  "routes/ActivityDetail.tsx": 23,
  "routes/PluginPage.tsx": 3,
  "routes/Segments.tsx": 27,
  "routes/Settings.tsx": 58,
};

/**
 * English the scanner cannot see — tables of names (sports, zones) and
 * strings a function returns. Kept by hand: each stage that translates one
 * takes it off, and the epic ends with this empty too.
 */
const TRACKED_BY_HAND: string[] = [
  "lib/errors.ts", // "Unknown error"
  "lib/gear.ts", // the Garage's rule summary and sensor names: "profile", "sensor", "Sensor"
  "lib/importSummary.ts", // "Imported", "Nothing imported"
  "stores/confirmStore.ts", // the default confirm label
];

describe("interface strings (#237)", () => {
  const files = sources(SRC).map((f) => [relative(SRC, f), untranslatedIn(f, readFileSync(f, "utf8"))] as const);

  it("come from the dictionaries outside the files not yet translated", () => {
    const offenders = files
      .filter(([f, found]) => found.length > 0 && !(f in NOT_YET_TRANSLATED))
      .flatMap(([f, found]) => found.map((s) => `${f}:${s}`));
    expect(offenders).toEqual([]);
  });

  it("are counted exactly in the files not yet translated", () => {
    const counts = Object.fromEntries(
      files.filter(([f]) => f in NOT_YET_TRANSLATED).map(([f, found]) => [f, found.length]),
    );
    expect(counts).toEqual(NOT_YET_TRANSLATED);
  });

  it("are tracked by hand only in files that exist", () => {
    expect(TRACKED_BY_HAND.filter((f) => !files.some(([g]) => g === f))).toEqual([]);
  });

  /** A component that calls the module-level `t` renders once in the
   * language of the moment and never again: components take `useT()`. */
  it("reach components through useT, never the module-level t", () => {
    const offenders = files
      .filter(([f]) => f.endsWith(".tsx"))
      .filter(([f]) =>
        /import\s*\{[^}]*\b(t|translate)\b[^}]*\}\s*from\s*["'][./]*(lib\/)?i18n["']/.test(readFileSync(join(SRC, f), "utf8")),
      )
      .map(([f]) => f);
    expect(offenders).toEqual([]);
  });
});

describe("the scanner", () => {
  it("finds English wherever an expression can put it on screen", () => {
    const source = [
      'const a = <p>Plain text</p>;',
      'const b = <p>{busy ? "Clearing…" : "Clear Cache"}</p>;',
      'const c = <p>{label ?? "Cancel"}</p>;',
      'const d = <p>{open && "Opened"}</p>;',
      'const e = <p>{"Failed: " + reason}</p>;',
      'const f = <p>{`Saved ${n} files`}</p>;',
      'const g = <b title={x ? "Tip one" : "Tip two"} aria-label="Close panel" />;',
      'const h = { label: "Distance", message: cond ? "Done" : "Oops", name: "ZIP Archive" };',
      'addToast("error", "Import failed");',
      'window.confirm("Delete it?");',
      'const i = <span translate="no">Wordmark</span>;',
      'const j = <p>Ignored</p>; // i18n-ignore',
      'const k = <p>{mode === "dark" ? icon : null}</p>;',
      'const l = <p>{t("settings.general.theme")}</p>;',
      'const m = <p>FTP · NP</p>;',
    ].join("\n");
    expect(untranslatedIn("fixture.tsx", source)).toEqual([
      "1: Plain text",
      "2: Clearing…",
      "2: Clear Cache",
      "3: Cancel",
      "4: Opened",
      "5: Failed:",
      "6: Saved files",
      "7: Tip one",
      "7: Tip two",
      "7: Close panel",
      "8: Distance",
      "8: Done",
      "8: Oops",
      "8: ZIP Archive",
      "9: Import failed",
      "10: Delete it?",
    ]);
  });
});
