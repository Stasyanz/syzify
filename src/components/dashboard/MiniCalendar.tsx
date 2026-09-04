import { useEffect } from "react";
import { useNavigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { api } from "../../lib/tauri";
import { getSportColor } from "../../lib/sportColors";
import { SportGlyph } from "../brand/SportIcon";
import { formatDistance, formatDuration } from "../../lib/format";
import { toDistance, distanceUnit, toElevation, elevationUnit, useUnits } from "../../lib/units";
import { SPORT_LABELS, type SportType, type DaySummary } from "../../lib/types";
import { WEEKDAYS, buildMonthGrid, monthSports } from "../../lib/calendar";
import { useMonthView, useToday } from "../../hooks/useToday";

/** Compact month calendar with activity dots + per-activity hover popups. */
export function MiniCalendar() {
  useUnits();
  // Live day: the "today" ring moves at midnight and the view follows a
  // month rollover while it shows the current month.
  const today = useToday();
  const navigate = useNavigate();
  const { year, month, setYear, setMonth } = useMonthView(today);

  const { data: days = [] } = useQuery({
    queryKey: ["calendar", year, month],
    queryFn: () => api.getCalendarData(year, month),
  });

  const dayMap = new Map<string, DaySummary>();
  for (const d of days) dayMap.set(d.date, d);

  // 6 weeks always, so the dashboard card keeps one height while the user
  // flips between 5- and 6-week months.
  const cells = buildMonthGrid(year, month, 6);

  const shift = (n: number) => {
    let m = month + n;
    let y = year;
    if (m < 1) { m = 12; y--; }
    if (m > 12) { m = 1; y++; }
    setMonth(m);
    setYear(y);
  };

  // Arrow keys page the month, same listener as the Library's calendar view
  // (typing targets excluded so the keys never hijack a form field).
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key === "ArrowLeft") shift(-1);
      if (e.key === "ArrowRight") shift(1);
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [month, year]);

  const monthLabel = new Date(year, month - 1).toLocaleString("en-US", {
    month: "long",
    year: "numeric",
  });

  const sessions = days.reduce((s, d) => s + d.activity_count, 0);
  const monthDistance = toDistance(days.reduce((s, d) => s + d.total_distance_m, 0));
  const monthElevGain = toElevation(days.reduce((s, d) => s + d.total_elev_gain_m, 0));
  // Same shape as the This Week Duration card: decimal hours + "h".
  const monthHours = days.reduce((s, d) => s + d.total_duration_s, 0) / 3600;
  const activeDays = days.length;

  return (
    <div className="dash-card">
      <div className="flex gap-5">
        <div className="flex-1 min-w-0">
          <h3 className="mb-0.5">Training calendar</h3>
          <div className="cal-head">
            <span />
            <div className="cal-title">{monthLabel}</div>
            <div className="cal-nav">
              <button onClick={() => shift(-1)} title="Previous month">
                <ChevronLeft size={16} />
              </button>
              <button onClick={() => shift(1)} title="Next month">
                <ChevronRight size={16} />
              </button>
            </div>
          </div>
          <div className="cal-grid">
            {WEEKDAYS.map((w) => (
              <div className="cal-dow" key={w}>
                {w}
              </div>
            ))}
            {cells.map((d, i) => {
              if (d === null) return <div className="cal-cell out" key={`e${i}`} />;
              const dateStr = `${year}-${String(month).padStart(2, "0")}-${String(d).padStart(2, "0")}`;
              const summary = dayMap.get(dateStr);
              const isToday =
                d === today.getDate() &&
                month === today.getMonth() + 1 &&
                year === today.getFullYear();
              const col = i % 7;
              const popCls = col <= 1 ? " pop-l" : col >= 5 ? " pop-r" : "";
              return (
                <div
                  className={`cal-cell${summary ? " has" : ""}${isToday ? " today" : ""}`}
                  key={i}
                >
                  <span className="cal-num">{d}</span>
                  <div className="cal-dots">
                    {summary?.activities.slice(0, 4).map((a) => (
                      <i key={a.id} style={{ background: getSportColor(a.sport_type) }} />
                    ))}
                  </div>

                  {/* Hover popup — each row links straight to the activity */}
                  {summary && (
                    <div className={`cal-pop${popCls}`}>
                      <div className="cal-pop-card">
                        {summary.activities.map((a) => (
                          <button
                            key={a.id}
                            className="cal-pop-row"
                            onClick={() => navigate(`/activity/${a.id}`)}
                          >
                            <span
                              className="cal-pop-ic"
                              style={{ background: getSportColor(a.sport_type) }}
                            >
                              <SportGlyph sport={a.sport_type} size={14} />
                            </span>
                            <span className="cal-pop-t">
                              {a.title ?? SPORT_LABELS[a.sport_type as SportType] ?? a.sport_type}
                            </span>
                            <span className="cal-pop-m">
                              {a.distance_m != null
                                ? formatDistance(a.distance_m)
                                : a.duration_s != null
                                  ? formatDuration(a.duration_s)
                                  : ""}
                            </span>
                            <ChevronRight size={14} className="cal-pop-arrow" />
                          </button>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>

        <div className="cal-aside">
          <div className="cal-aside-h">This month</div>
          <div className="cal-sum">
            <div>
              <div className="k">{sessions}</div>
              <div className="l">Sessions</div>
            </div>
            <div>
              <div className="k">
                {monthDistance.toFixed(0)}
                <span>{distanceUnit()}</span>
              </div>
              <div className="l">Distance</div>
            </div>
            {/* No "h" suffix — the "Hours" label below already names the unit
                (unlike Distance/Elev gain, whose labels don't carry km/m). */}
            <div>
              <div className="k">{monthHours.toFixed(1)}</div>
              <div className="l">Hours</div>
            </div>
            {/* Hidden while zero: a flat-terrain (or elevation-less GPX)
                month would otherwise pin a dead "0 m" row to the card. */}
            {monthElevGain > 0 && (
              <div>
                <div className="k">
                  {Math.round(monthElevGain).toLocaleString("en-US")}
                  <span>{elevationUnit()}</span>
                </div>
                <div className="l">Elev gain</div>
              </div>
            )}
            <div>
              <div className="k">{activeDays}</div>
              <div className="l">Active days</div>
            </div>
          </div>
          {/* Only the sports actually present in the shown month */}
          <div className="cal-legend">
            {monthSports(days).map((s) => (
              <div key={s}>
                <i style={{ background: getSportColor(s) }} />
                {SPORT_LABELS[s as SportType] ?? s}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
