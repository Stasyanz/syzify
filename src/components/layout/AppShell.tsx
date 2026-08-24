import type { ReactNode } from "react";
import { useNavigate, useLocation } from "react-router";
import {
  PieChart,
  Calendar,
  List,
  Map as MapIcon,
  Route as RouteIcon,
  SlidersHorizontal,
  Settings,
  Sun,
  Moon,
} from "lucide-react";
import { Logo } from "../brand/Logo";
import { FilterDrawer, countActiveFilters } from "../library/FilterDrawer";
import { ImportDialog } from "../import/ImportDialog";
import { OnboardingOverlay } from "./OnboardingOverlay";
import { useActivityStore } from "../../stores/activityStore";
import { useThemeStore, resolveDark, systemPrefersDark } from "../../lib/theme";

/**
 * Trailhead app shell — navbar with a centered icon-only nav-rail and a right
 * cluster (filter / theme / settings). Mirrors the claude.ai/design layout:
 * the wordmark and the right cluster both flex:1, which centers the rail.
 * Calendar/Library/Map are view modes of the activity library; Dashboard is
 * its own route. The filter button (and right drawer) only show on the library
 * views and toggle `activityStore.filtersOpen`.
 */
type RailItem =
  | { key: string; label: string; icon: typeof List; path: string }
  | { key: string; label: string; icon: typeof List; view: "list" | "calendar" | "map" };

const RAIL: RailItem[] = [
  { key: "dashboard", label: "Dashboard", icon: PieChart, path: "/" },
  { key: "calendar", label: "Calendar", icon: Calendar, view: "calendar" },
  { key: "library", label: "Library", icon: List, view: "list" },
  { key: "map", label: "Map", icon: MapIcon, view: "map" },
  { key: "segments", label: "Segments", icon: RouteIcon, path: "/segments" },
];

export function AppShell({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const viewMode = useActivityStore((s) => s.viewMode);
  const setViewMode = useActivityStore((s) => s.setViewMode);
  const filtersOpen = useActivityStore((s) => s.filtersOpen);
  const toggleFilters = useActivityStore((s) => s.toggleFilters);
  const filters = useActivityStore((s) => s.filters);
  const mode = useThemeStore((s) => s.mode);
  const setMode = useThemeStore((s) => s.setMode);

  const isDark = resolveDark(mode, systemPrefersDark());
  const onLibrary = pathname === "/library";
  const activeFilters = countActiveFilters(filters);

  const isActive = (item: RailItem) =>
    "path" in item ? pathname === item.path : onLibrary && viewMode === item.view;

  const go = (item: RailItem) => {
    if ("path" in item) {
      navigate(item.path);
    } else {
      if (!onLibrary) navigate("/library");
      setViewMode(item.view);
    }
  };

  // macOS overlay titlebar reserves navbar space for the native traffic lights.
  const isMac = navigator.userAgent.includes("Mac");

  return (
    <div className="h-screen flex flex-col bg-bg text-ink overflow-hidden">
      {/* Drag attributes sit on every flex container that owns the navbar's
          empty space — the injected Tauri handler only fires when the
          mousedown target itself carries the attribute, so buttons inside
          stay clickable. */}
      <div className={`navbar${isMac ? " mac-overlay" : ""}`} data-tauri-drag-region>
        <div className="wordmark" data-tauri-drag-region>
          <button
            onClick={() => navigate("/")}
            title="Syzify"
            className="-mt-[3px]"
          >
            <Logo size={24} />
          </button>
        </div>

        <div className="rail">
          {RAIL.map((item) => {
            const Icon = item.icon;
            return (
              <div
                key={item.key}
                className={`ri${isActive(item) ? " on" : ""}`}
                onClick={() => go(item)}
                title={item.label}
              >
                <Icon size={20} />
              </div>
            );
          })}
        </div>

        <div className="navbar-right" data-tauri-drag-region>
          <ImportDialog variant="icon" />
          {onLibrary && (
            <button
              className={`filter-icon-btn${filtersOpen ? " on" : ""}`}
              onClick={toggleFilters}
              data-tip="Filters"
              aria-label="Filters"
            >
              <SlidersHorizontal size={17} />
              {activeFilters > 0 && !filtersOpen && (
                <span className="ft-badge">{activeFilters}</span>
              )}
            </button>
          )}
          <button
            className="themebtn"
            onClick={() => setMode(isDark ? "light" : "dark")}
            data-tip={isDark ? "Switch to light" : "Switch to dark"}
            aria-label={isDark ? "Switch to light" : "Switch to dark"}
          >
            {isDark ? <Sun size={18} /> : <Moon size={18} />}
          </button>
          <div
            className={`ri tip-left${pathname === "/settings" ? " on" : ""}`}
            onClick={() => navigate("/settings")}
            data-tip="Settings"
            aria-label="Settings"
          >
            <Settings size={20} />
          </div>
        </div>
      </div>

      <main className="relative flex flex-1 min-h-0 flex-col overflow-hidden">
        {children}
        {filtersOpen && onLibrary && <FilterDrawer />}
      </main>
      <OnboardingOverlay />
    </div>
  );
}
