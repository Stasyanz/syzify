import { ActivityList } from "../components/library/ActivityList";
import { CalendarView } from "../components/library/CalendarView";
import { ActivitiesMap } from "../components/library/ActivitiesMap";
import { useActivityStore } from "../stores/activityStore";

// Filtering lives in the right-hand FilterDrawer (toggled from the navbar);
// the active filters are read straight from the store by each view.
export function Library() {
  const viewMode = useActivityStore((s) => s.viewMode);

  return (
    <div className={viewMode === "map" ? "h-full min-h-0" : "h-full overflow-y-auto scroll-themed"}>
      {viewMode === "map" ? (
        <ActivitiesMap />
      ) : viewMode === "calendar" ? (
        <CalendarView />
      ) : (
        <ActivityList />
      )}
    </div>
  );
}
