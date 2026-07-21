import { useState, useCallback, useEffect } from "react";
import { useNavigate } from "react-router";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { api } from "../../lib/tauri";
import { useActivityStore } from "../../stores/activityStore";
import { formatDistance, formatDuration } from "../../lib/format";
import { useUnits } from "../../lib/units";
import { SportGlyph } from "../brand/SportIcon";
import { getSportColor } from "../../lib/sportColors";
import { SPORT_LABELS, type SportType, type DaySummary } from "../../lib/types";
import { WEEKDAYS, buildMonthGrid } from "../../lib/calendar";

export function CalendarView() {
  useUnits();
  const today = new Date();
  const navigate = useNavigate();
  const [year, setYear] = useState(today.getFullYear());
  const [month, setMonth] = useState(today.getMonth() + 1);
  const filters = useActivityStore((s) => s.filters);

  const { data: days = [] } = useQuery({
    queryKey: ["calendar", year, month, filters],
    queryFn: () => api.getCalendarData(year, month, filters),
  });

  const dayMap = new Map<string, DaySummary>();
  for (const d of days) {
    dayMap.set(d.date, d);
  }

  const prevMonth = useCallback(() => {
    if (month === 1) {
      setYear(year - 1);
      setMonth(12);
    } else {
      setMonth(month - 1);
    }
  }, [month, year]);

  const nextMonth = useCallback(() => {
    if (month === 12) {
      setYear(year + 1);
      setMonth(1);
    } else {
      setMonth(month + 1);
    }
  }, [month, year]);

  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      const tag = (e.target as HTMLElement).tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
      if (e.key === "ArrowLeft") prevMonth();
      if (e.key === "ArrowRight") nextMonth();
    }
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [prevMonth, nextMonth]);


  const monthLabel = new Date(year, month - 1).toLocaleString(undefined, {
    month: "long",
    year: "numeric",
  });

  const cells = buildMonthGrid(year, month);

  return (
    <div className="p-4">
      {/* Header — month centered, both nav buttons grouped on the right
          (matches the dashboard MiniCalendar). */}
      <div className="cal-head mb-4">
        <span />
        <h2 className="cal-title capitalize">{monthLabel}</h2>
        <div className="cal-nav">
          <button onClick={prevMonth} title="Previous month">
            <ChevronLeft size={18} />
          </button>
          <button onClick={nextMonth} title="Next month">
            <ChevronRight size={18} />
          </button>
        </div>
      </div>

      {/* Grid (overflow visible so day popups can escape the cells) */}
      <div className="grid grid-cols-7 gap-1.5">
        {WEEKDAYS.map((wd) => (
          <div key={wd} className="cal-dow">
            {wd}
          </div>
        ))}

        {cells.map((day, idx) => {
          if (day === null) {
            return <div key={`empty-${idx}`} className="min-h-[80px]" />;
          }

          const dateStr = `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
          const summary = dayMap.get(dateStr);
          const isToday =
            day === today.getDate() &&
            month === today.getMonth() + 1 &&
            year === today.getFullYear();

          const col = idx % 7;
          const popAlign =
            col <= 1 ? "left-0 translate-x-0" : col >= 5 ? "left-auto right-0 translate-x-0" : "";

          return (
            <div
              key={day}
              className={`group relative min-h-[80px] p-1.5 rounded-lg border transition-colors hover:border-accent ${
                summary ? "bg-accent-soft border-border" : "bg-card-2 border-border"
              } ${isToday ? "ring-2 ring-inset ring-accent" : ""}`}
            >
              <div
                className={`text-xs font-medium mb-1 ${
                  isToday ? "text-accent-2" : "text-ink"
                }`}
              >
                {day}
              </div>

              {/* Activity pills — one per workout, colored by sport */}
              {summary && (
                <div className="cal-titles">
                  {summary.activities.slice(0, 4).map((a) => (
                    <span
                      key={a.id}
                      className="cal-pill"
                      style={{ background: getSportColor(a.sport_type) }}
                      title={a.title ?? SPORT_LABELS[a.sport_type as SportType] ?? a.sport_type}
                      onClick={(e) => {
                        e.stopPropagation();
                        navigate(`/activity/${a.id}`);
                      }}
                    >
                      <SportGlyph sport={a.sport_type} size={11} />
                      <span className="cal-pill-t">
                        {a.title ?? SPORT_LABELS[a.sport_type as SportType] ?? a.sport_type}
                      </span>
                    </span>
                  ))}
                  {summary.activities.length > 4 && (
                    <span className="text-[10px] text-faint leading-none pl-0.5">
                      +{summary.activities.length - 4}
                    </span>
                  )}
                </div>
              )}

              {/* Hover popup — one row per workout, opens the activity */}
              {summary && (
                <div
                  className={`absolute bottom-full left-1/2 -translate-x-1/2 z-50 hidden w-[230px] pb-2 group-hover:block ${popAlign}`}
                >
                  <div className="cal-pop-card">
                    {summary.activities.map((a) => (
                      <button
                        key={a.id}
                        className="cal-pop-row"
                        onClick={(e) => {
                          e.stopPropagation();
                          navigate(`/activity/${a.id}`);
                        }}
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
  );
}
