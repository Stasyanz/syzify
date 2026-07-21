// @vitest-environment happy-dom
//
// Invariant: the map / elevation blocks drawn onto the export canvas and the
// ones rendered in the on-photo SVG preview come from ONE layout source
// (shareGeometry.ts) and are therefore pixel-identical (WYSIWYG). If a future
// edit reintroduces a private formula in either renderer, the path strings /
// sizes diverge from computeMap/ElevationLayout and this test fails.
import { describe, it, expect, beforeAll, afterEach } from "vitest";
import { render, cleanup } from "@testing-library/react";
import type { Activity, TrackPointColumns } from "../../lib/types";
import { DraggableShareBlocks } from "./DraggableShareBlocks";
import {
  drawMapBlock,
  drawElevationBlock,
  drawBrandMark,
  BRAND_MARK_PATH,
  BRAND_ACCENT,
  BRAND_WORDMARK,
  BRAND_WORDMARK_TYPESET,
  type DrawCtx,
} from "./shareCanvas";
import { computeMapLayout, computeElevationLayout } from "./shareGeometry";

const ROUTE = [
  { lat: 50.0, lon: 30.0 },
  { lat: 50.01, lon: 30.02 },
  { lat: 50.02, lon: 30.01 },
  { lat: 50.015, lon: 30.03 },
];
const ALTS = [100, 120, 110, 140, 130, 160];

const EXPORT_W = 1920;
const EXPORT_H = 1080;
const PREVIEW_W = 800; // a strong-crop-ish downscale
const K = PREVIEW_W / EXPORT_W;
const MAP_POS = { x: 0.66, y: 0.58 };
const ELEV_POS = { x: 0.66, y: 0.84 };

const trackpoints = {
  lat: ROUTE.map((p) => p.lat),
  lon: ROUTE.map((p) => p.lon),
  altitude_m: ALTS,
} as unknown as TrackPointColumns;

const activity = {
  id: "act-1",
  start_time: "2026-05-01T08:00:00+00:00",
  sport_type: "run",
} as unknown as Activity;

const positions = {
  title: { x: 0, y: 0 },
  metrics: { x: 0, y: 0 },
  map: MAP_POS,
  elevation: ELEV_POS,
};

const SCALES_1 = { title: 1, metrics: 1, map: 1, elevation: 1 };

// Minimal fake 2D context + Path2D that record the geometry the canvas renderer
// commits, without needing a real canvas (none in happy-dom; avoids a dep).
function makeFakeCtx() {
  const calls = {
    translate: null as null | { x: number; y: number },
    strokePaths: [] as { d: string; lineWidth: number }[],
    fillPaths: [] as { d: string; alpha: number; fillStyle: string }[],
    arcs: [] as { x: number; y: number; r: number }[],
    // fillStyle at each no-Path2D fill() — the chip plate and the route dots.
    plainFills: [] as string[],
    texts: [] as string[],
  };
  const ctx = {
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 0,
    lineJoin: "",
    lineCap: "",
    globalAlpha: 1,
    font: "",
    save() {},
    restore() {},
    beginPath() {},
    moveTo() {},
    lineTo() {},
    arcTo() {},
    closePath() {},
    scale() {},
    measureText: () => ({ width: 60 }),
    fillText(text: string) {
      calls.texts.push(text);
    },
    translate(x: number, y: number) {
      calls.translate = { x, y };
    },
    arc(x: number, y: number, r: number) {
      calls.arcs.push({ x, y, r });
    },
    fill(p?: { d: string }) {
      if (p) calls.fillPaths.push({ d: p.d, alpha: ctx.globalAlpha, fillStyle: String(ctx.fillStyle) });
      else calls.plainFills.push(String(ctx.fillStyle));
    },
    stroke(p?: { d: string }) {
      if (p) calls.strokePaths.push({ d: p.d, lineWidth: ctx.lineWidth });
    },
  };
  return { ctx, calls };
}

beforeAll(() => {
  // happy-dom has no Path2D; capture its source string.
  (globalThis as unknown as { Path2D: unknown }).Path2D = class {
    d: string;
    constructor(d: string) {
      this.d = d;
    }
  };
});

afterEach(cleanup);

function renderBlocks(showMap: boolean, showElevation: boolean, scales = SCALES_1) {
  return render(
    <DraggableShareBlocks
      activity={activity}
      trackpoints={trackpoints}
      fields={[]}
      theme="dark"
      showTitle
      showMap={showMap}
      showElevation={showElevation}
      positions={positions}
      onPositionChange={() => {}}
      scales={scales}
      onScaleChange={() => {}}
      previewWidth={PREVIEW_W}
      previewHeight={(PREVIEW_W * EXPORT_H) / EXPORT_W}
      exportWidth={EXPORT_W}
    />
  );
}

describe("crop WYSIWYG — canvas and SVG share one layout", () => {
  it("map: canvas paths/markers/sizes == SVG == computeMapLayout", () => {
    const L = computeMapLayout(EXPORT_W, ROUTE)!;
    expect(L).not.toBeNull();

    // --- canvas side ---
    const { ctx, calls } = makeFakeCtx();
    const d: DrawCtx = { ctx: ctx as unknown as CanvasRenderingContext2D, W: EXPORT_W, H: EXPORT_H, theme: "dark", baseFs: 0 };
    drawMapBlock(d, MAP_POS, trackpoints);

    // route polyline drawn via Path2D(L.d), translated to the chip's inner origin
    expect(calls.strokePaths).toHaveLength(1);
    expect(calls.strokePaths[0].d).toBe(L.d);
    expect(calls.strokePaths[0].lineWidth).toBeCloseTo(Math.max(2, L.innerW * 0.012), 6);
    expect(calls.translate).toEqual({ x: MAP_POS.x * EXPORT_W + L.padding, y: MAP_POS.y * EXPORT_H + L.padding });
    // start / end markers at the shared layout's points
    expect(calls.arcs).toHaveLength(2);
    expect(calls.arcs[0].x).toBeCloseTo(L.start.x, 6);
    expect(calls.arcs[0].y).toBeCloseTo(L.start.y, 6);
    expect(calls.arcs[1].x).toBeCloseTo(L.end.x, 6);
    expect(calls.arcs[1].y).toBeCloseTo(L.end.y, 6);

    // --- SVG side ---
    const { container } = renderBlocks(true, false);
    const svg = container.querySelector("svg")!;
    const path = svg.querySelector("path")!;
    const circles = svg.querySelectorAll("circle");

    // same path string as the canvas and the shared function — the invariant
    expect(path.getAttribute("d")).toBe(L.d);
    expect(path.getAttribute("d")).toBe(calls.strokePaths[0].d);
    // viewBox is in export-px; only the on-screen width scales by k
    expect(svg.getAttribute("viewBox")).toBe(`0 0 ${L.innerW} ${L.innerH}`);
    expect(Number(svg.getAttribute("width"))).toBeCloseTo(L.innerW * K, 4);
    expect(Number(svg.getAttribute("height"))).toBeCloseTo(L.innerH * K, 4);
    // stroke width given in export-px on both sides → identical on-screen after the uniform k
    expect(Number(path.getAttribute("stroke-width"))).toBeCloseTo(calls.strokePaths[0].lineWidth, 6);
    // markers at the same shared points
    expect(Number(circles[0].getAttribute("cx"))).toBeCloseTo(L.start.x, 6);
    expect(Number(circles[0].getAttribute("cy"))).toBeCloseTo(L.start.y, 6);
    expect(Number(circles[1].getAttribute("cx"))).toBeCloseTo(L.end.x, 6);
    expect(Number(circles[1].getAttribute("cy"))).toBeCloseTo(L.end.y, 6);
  });

  it("elevation: canvas fill/line paths == SVG == computeElevationLayout", () => {
    const L = computeElevationLayout(EXPORT_W, ALTS)!;
    expect(L).not.toBeNull();

    // --- canvas side ---
    const { ctx, calls } = makeFakeCtx();
    const d: DrawCtx = { ctx: ctx as unknown as CanvasRenderingContext2D, W: EXPORT_W, H: EXPORT_H, theme: "dark", baseFs: 0 };
    drawElevationBlock(d, ELEV_POS, trackpoints);

    expect(calls.fillPaths).toHaveLength(1);
    expect(calls.fillPaths[0].d).toBe(L.fillD);
    expect(calls.fillPaths[0].alpha).toBeCloseTo(0.18, 6);
    expect(calls.strokePaths).toHaveLength(1);
    expect(calls.strokePaths[0].d).toBe(L.lineD);
    expect(calls.strokePaths[0].lineWidth).toBeCloseTo(Math.max(2, L.innerW * 0.008), 6);
    expect(calls.translate).toEqual({ x: ELEV_POS.x * EXPORT_W + L.padding, y: ELEV_POS.y * EXPORT_H + L.padding });

    // --- SVG side ---
    const { container } = renderBlocks(false, true);
    const svg = container.querySelector("svg")!;
    const paths = svg.querySelectorAll("path");
    const fillPath = paths[0];
    const linePath = paths[1];

    expect(fillPath.getAttribute("d")).toBe(L.fillD);
    expect(fillPath.getAttribute("d")).toBe(calls.fillPaths[0].d);
    expect(Number(fillPath.getAttribute("fill-opacity"))).toBeCloseTo(0.18, 6);
    expect(linePath.getAttribute("d")).toBe(L.lineD);
    expect(linePath.getAttribute("d")).toBe(calls.strokePaths[0].d);
    expect(svg.getAttribute("viewBox")).toBe(`0 0 ${L.innerW} ${L.innerH}`);
    expect(Number(svg.getAttribute("width"))).toBeCloseTo(L.innerW * K, 4);
    expect(Number(linePath.getAttribute("stroke-width"))).toBeCloseTo(calls.strokePaths[0].lineWidth, 6);
  });
});

describe("block scale — canvas and SVG stay WYSIWYG", () => {
  it("map at scale 2: both renderers use computeMapLayout(2·exportW)", () => {
    const L2 = computeMapLayout(EXPORT_W * 2, ROUTE)!;

    // --- canvas side ---
    const { ctx, calls } = makeFakeCtx();
    const d: DrawCtx = { ctx: ctx as unknown as CanvasRenderingContext2D, W: EXPORT_W, H: EXPORT_H, theme: "dark", baseFs: 0 };
    drawMapBlock(d, MAP_POS, trackpoints, 2);
    expect(calls.strokePaths[0].d).toBe(L2.d);
    expect(calls.strokePaths[0].lineWidth).toBeCloseTo(Math.max(2, L2.innerW * 0.012), 6);

    // --- SVG side ---
    const { container } = renderBlocks(true, false, { ...SCALES_1, map: 2 });
    const svg = container.querySelector("svg")!;
    expect(svg.querySelector("path")!.getAttribute("d")).toBe(L2.d);
    expect(svg.getAttribute("viewBox")).toBe(`0 0 ${L2.innerW} ${L2.innerH}`);
  });
});

const DARK_CHIP_BG = "rgba(0,0,0,0.55)";

describe("transparent block background — canvas and preview stay WYSIWYG", () => {
  it("canvas: the chip plate is skipped and no glyph halo is set", () => {
    // Default: the plate is filled with the theme chip color.
    {
      const { ctx, calls } = makeFakeCtx();
      const d: DrawCtx = { ctx: ctx as unknown as CanvasRenderingContext2D, W: EXPORT_W, H: EXPORT_H, theme: "dark", baseFs: 0 };
      drawMapBlock(d, MAP_POS, trackpoints);
      expect(calls.plainFills).toContain(DARK_CHIP_BG);
    }
    // Transparent: no plate fill, no shadow either, route still drawn.
    {
      const { ctx, calls } = makeFakeCtx();
      const d: DrawCtx = {
        ctx: ctx as unknown as CanvasRenderingContext2D,
        W: EXPORT_W,
        H: EXPORT_H,
        theme: "dark",
        baseFs: 0,
        transparentBg: true,
      };
      drawMapBlock(d, MAP_POS, trackpoints);
      expect(calls.plainFills).not.toContain(DARK_CHIP_BG);
      expect((ctx as unknown as { shadowColor?: string }).shadowColor).toBeUndefined();
      expect(calls.strokePaths).toHaveLength(1);
      expect(calls.arcs).toHaveLength(2); // start/end dots survive
    }
  });

  it("canvas: elevation drops both the plate AND the translucent area fill", () => {
    const { ctx, calls } = makeFakeCtx();
    const d: DrawCtx = {
      ctx: ctx as unknown as CanvasRenderingContext2D,
      W: EXPORT_W,
      H: EXPORT_H,
      theme: "dark",
      baseFs: 0,
      transparentBg: true,
    };
    drawElevationBlock(d, ELEV_POS, trackpoints);
    // No plate, no under-curve veil — only the line remains.
    expect(calls.plainFills).toHaveLength(0);
    expect(calls.fillPaths).toHaveLength(0);
    expect(calls.strokePaths).toHaveLength(1);
    expect(calls.strokePaths[0].d).toBe(computeElevationLayout(EXPORT_W, ALTS)!.lineD);
  });

  it("preview: chips lose the plate without gaining a halo", () => {
    const { container } = render(
      <DraggableShareBlocks
        activity={activity}
        trackpoints={trackpoints}
        fields={[{ key: "distance", label: "Distance", value: "10 km" }]}
        theme="dark"
        showTitle
        showMap
        showElevation
        positions={positions}
        onPositionChange={() => {}}
        scales={SCALES_1}
        onScaleChange={() => {}}
        previewWidth={PREVIEW_W}
        previewHeight={(PREVIEW_W * EXPORT_H) / EXPORT_W}
        exportWidth={EXPORT_W}
        transparentBg
      />
    );

    // Map + elevation + brand mark: no plate on the container, no halo on
    // any SVG.
    const svgs = container.querySelectorAll("svg");
    expect(svgs).toHaveLength(3);
    for (const svg of Array.from(svgs)) {
      expect((svg.parentElement as HTMLElement).style.background).toBe("");
      expect((svg as unknown as { style: CSSStyleDeclaration }).style.filter).toBe("");
    }
    // Elevation (the second chip) loses its area-fill path too — line only.
    expect(svgs[1].querySelectorAll("path")).toHaveLength(1);

    // No text-shadow halo anywhere — neither text chips nor the brand mark.
    const halos = Array.from(container.querySelectorAll("div")).filter(
      (el) => el.style.textShadow !== ""
    );
    expect(halos).toHaveLength(0);
  });
});

describe("title block visibility", () => {
  function renderWithTitle(showTitle: boolean) {
    return render(
      <DraggableShareBlocks
        activity={activity}
        trackpoints={trackpoints}
        fields={[]}
        theme="dark"
        showTitle={showTitle}
        showMap={false}
        showElevation={false}
        positions={positions}
        onPositionChange={() => {}}
        scales={SCALES_1}
        onScaleChange={() => {}}
        previewWidth={PREVIEW_W}
        previewHeight={(PREVIEW_W * EXPORT_H) / EXPORT_W}
        exportWidth={EXPORT_W}
      />
    );
  }

  it("showTitle=false removes the title chip from the preview", () => {
    // The block shows "RUN · <date>" for this activity; with the toggle off
    // nothing of it must render — only the fixed brand watermark stays.
    const on = renderWithTitle(true);
    expect(on.container.textContent).toContain("RUN");
    cleanup();
    const off = renderWithTitle(false);
    expect(off.container.textContent).not.toContain("RUN");
    // The visible text uses the dotless-ı typeset form (the accent dot is
    // drawn, not typed); the accessible name stays the clean BRAND_WORDMARK.
    expect(off.container.textContent).toBe(BRAND_WORDMARK_TYPESET);
    expect(off.container.querySelector(`[aria-label="${BRAND_WORDMARK}"]`)).toBeTruthy();
  });
});

describe("brand watermark", () => {
  it("canvas: drawn regardless of the transparent-background option", () => {
    for (const transparentBg of [false, true]) {
      const { ctx, calls } = makeFakeCtx();
      const d: DrawCtx = {
        ctx: ctx as unknown as CanvasRenderingContext2D,
        W: EXPORT_W,
        H: EXPORT_H,
        theme: "dark",
        baseFs: 20,
        transparentBg,
      };
      drawBrandMark(d);
      expect(calls.fillPaths.map((p) => p.d)).toContain(BRAND_MARK_PATH);
      expect(calls.texts).toContain(BRAND_WORDMARK_TYPESET);
      expect(calls.arcs).toHaveLength(2); // the boulder dot + the i's dot
      // The ridge is theme ink (mirrors Logo.tsx's var(--ink)), NOT the
      // accent left over from the i-dot — the regression that shipped an
      // all-terracotta mark whose own boulder vanished into it.
      const mark = calls.fillPaths.find((p) => p.d === BRAND_MARK_PATH)!;
      expect(mark.fillStyle).toBe("#ffffff");
      // Both dots stay accent (they're the plain no-Path2D fills).
      expect(calls.plainFills).toEqual([BRAND_ACCENT, BRAND_ACCENT]);
    }
  });

  it("preview: present with every toggleable block switched off", () => {
    const { container } = renderBlocks(false, false); // fields=[] and no title-off path here…
    // …but the watermark is unconditional either way.
    expect(container.textContent).toContain(BRAND_WORDMARK_TYPESET);
    const svg = container.querySelector("svg")!;
    expect(svg.querySelector("path")!.getAttribute("d")).toBe(BRAND_MARK_PATH);
    // Not interactive: clicks/drags fall through to whatever is underneath.
    expect((svg.parentElement as HTMLElement).style.pointerEvents).toBe("none");
  });
});
