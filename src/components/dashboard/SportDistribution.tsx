import { useState } from "react";
import type { SportShare } from "../../lib/types";
import { getSportColor } from "../../lib/sportColors";
import { SPORT_LABELS, type SportType } from "../../lib/types";

interface Props {
  /** Last-7-days sport split (5 busiest, shares sum to 100), from the backend. */
  distribution: SportShare[];
}

const sportLabel = (sport: string) => SPORT_LABELS[sport as SportType] ?? sport;

export function SportDistribution({ distribution }: Props) {
  // Index of the donut segment under the cursor.
  const [hover, setHover] = useState<number | null>(null);

  if (distribution.length === 0) {
    return (
      <div className="dash-card">
        <h3>By sport</h3>
        <div className="text-[11px] font-medium text-faint mt-0.5">Last 7 days</div>
        <div className="h-48 flex items-center justify-center text-sm text-faint">
          No activities
        </div>
      </div>
    );
  }

  const total = distribution.reduce((sum, e) => sum + e.activities, 0);

  let acc = 0;
  const segments = distribution.map((e, i) => {
    const share = e.share_pct;
    const start = acc;
    acc += share;
    return { sport_type: e.sport_type, i, share, start };
  });

  const hovered = hover != null ? segments[hover] : undefined;

  return (
    <div className="dash-card">
      <h3>By sport</h3>
      <div className="text-[11px] font-medium text-faint mt-0.5">Last 7 days</div>

      {/* Donut centered in the middle of the card. Stroked SVG arcs (one per
          sport) + center hole; the hovered arc is drawn last and thicker so it
          grows above its neighbours. */}
      <div className="flex-1 grid place-items-center py-3">
        <div className="relative" style={{ width: 104, height: 104 }}>
          <svg
            viewBox="0 0 120 120"
            width={104}
            height={104}
            style={{ transform: "rotate(-90deg)" }}
          >
            {segments
              .filter((s) => s.i !== hover)
              .concat(hovered ? [hovered] : [])
              .map((s) => (
                <circle
                  key={s.sport_type}
                  cx={60}
                  cy={60}
                  r={44}
                  fill="none"
                  stroke={getSportColor(s.sport_type)}
                  strokeWidth={hover === s.i ? 28 : 18}
                  pathLength={100}
                  strokeDasharray={`${s.share} ${100 - s.share}`}
                  strokeDashoffset={-s.start}
                  style={{
                    cursor: "pointer",
                    opacity: hover == null || hover === s.i ? 1 : 0.3,
                    transition: "opacity 140ms ease, stroke-width 140ms ease",
                  }}
                  onMouseEnter={() => setHover(s.i)}
                  onMouseLeave={() => setHover(null)}
                />
              ))}
          </svg>

          {/* Center: hovered sport + share, or the activity total */}
          <div className="absolute inset-0 grid place-items-center pointer-events-none">
            {hovered ? (
              <div className="text-center px-2">
                <div className="font-num font-extrabold leading-none" style={{ fontSize: 18 }}>
                  {hovered.share}%
                </div>
                <div className="text-muted leading-tight mt-0.5" style={{ fontSize: 9 }}>
                  {sportLabel(hovered.sport_type)}
                </div>
              </div>
            ) : (
              <div className="text-center">
                <div className="font-num font-extrabold leading-none" style={{ fontSize: 19 }}>
                  {total}
                </div>
                <div className="text-faint" style={{ fontSize: 9 }}>
                  activities
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Sport names at the bottom (Volume-style legend): static, no hover. */}
      <div className="flex flex-wrap gap-x-4 gap-y-2 mt-4 text-xs text-muted">
        {segments.map((s) => (
          <span key={s.sport_type} className="flex items-center gap-1.5">
            <span
              className="w-2.5 h-2.5 shrink-0 rounded-[3px]"
              style={{ background: getSportColor(s.sport_type) }}
            />
            <span>{sportLabel(s.sport_type)}</span>
          </span>
        ))}
      </div>
    </div>
  );
}
