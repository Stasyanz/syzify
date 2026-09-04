import { useQuery } from "@tanstack/react-query";
import { api } from "../lib/tauri";
import { SummaryCards } from "../components/dashboard/SummaryCards";
import { VolumeChart } from "../components/dashboard/VolumeChart";
import { SportDistribution } from "../components/dashboard/SportDistribution";
import { PersonalRecords } from "../components/dashboard/PersonalRecords";
import { MiniCalendar } from "../components/dashboard/MiniCalendar";
import { PluginContributions } from "../components/plugins/PluginContributions";
import { useInvalidateOnNewDay } from "../hooks/useToday";
import "../lib/chartSetup";

export function DashboardPage() {
  const { data, isLoading } = useQuery({
    queryKey: ["dashboard"],
    queryFn: () => api.getDashboardData(),
  });
  // The backend cuts "this week" / "last 7 days" at ITS clock when asked
  // (date('now','localtime')), so the answer is only good for the day it was
  // fetched on — refetch past midnight, keeping the page on screen meanwhile.
  useInvalidateOnNewDay(["dashboard"]);

  return (
    <div className="flex flex-col h-full">
      <main className="flex-1 overflow-y-auto scroll-themed">
        {isLoading ? (
          <div className="flex items-center justify-center h-64">
            <p className="text-sm text-faint">Loading dashboard...</p>
          </div>
        ) : data ? (
          <div className="p-6 space-y-5">
            <SummaryCards data={data} />
            <div className="threecol">
              <VolumeChart weekVolume={data.week_volume} />
              <SportDistribution distribution={data.week_sport_distribution} />
              <PersonalRecords recordsBySport={data.records_by_sport} />
            </div>
            <MiniCalendar />
            <PluginContributions point="dashboard.widget" />
          </div>
        ) : (
          <div className="flex items-center justify-center h-64">
            <p className="text-sm text-faint">No data available</p>
          </div>
        )}
      </main>
    </div>
  );
}
