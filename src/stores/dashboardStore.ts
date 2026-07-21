import { create } from "zustand";

// NOTE: the dashboard shows fixed windows (this week / this month / all-time
// records) — there is deliberately no period state here. The backend's
// get_dashboard_data(period) stays period-aware for the plugin host API.
interface DashboardStoreState {
  volumeMetric: "distance" | "duration";
  setVolumeMetric: (metric: "distance" | "duration") => void;
}

export const useDashboardStore = create<DashboardStoreState>((set) => ({
  volumeMetric: "distance",
  setVolumeMetric: (volumeMetric) => set({ volumeMetric }),
}));
