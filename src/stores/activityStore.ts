import { create } from "zustand";
import type { ActivityFilters } from "../lib/types";

interface ActivityStoreState {
  filters: ActivityFilters;
  viewMode: "list" | "calendar" | "map";
  hoveredPointIndex: number | null;
  filtersOpen: boolean;
  setFilters: (filters: Partial<ActivityFilters>) => void;
  resetFilters: () => void;
  setViewMode: (mode: "list" | "calendar" | "map") => void;
  setHoveredPointIndex: (index: number | null) => void;
  setFiltersOpen: (open: boolean) => void;
  toggleFilters: () => void;
}

const defaultFilters: ActivityFilters = {
  sort_by: "date",
  sort_dir: "desc",
  limit: 20,
  offset: 0,
};

export const useActivityStore = create<ActivityStoreState>((set) => ({
  filters: defaultFilters,
  viewMode: "list",
  hoveredPointIndex: null,
  filtersOpen: false,

  setFilters: (partial) =>
    set((state) => ({
      filters: { ...state.filters, ...partial },
    })),

  resetFilters: () => set({ filters: defaultFilters }),

  setViewMode: (mode) => set({ viewMode: mode }),

  setHoveredPointIndex: (index) => set({ hoveredPointIndex: index }),

  setFiltersOpen: (open) => set({ filtersOpen: open }),

  toggleFilters: () => set((state) => ({ filtersOpen: !state.filtersOpen })),
}));
