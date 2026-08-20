import { useEffect, useState } from "react";
import { BrowserRouter, Routes, Route } from "react-router";
import { QueryClient, QueryClientProvider, useQuery, useQueryClient } from "@tanstack/react-query";
import { Library } from "./routes/Library";
import { DashboardPage } from "./routes/Dashboard";
import { ActivityDetailPage } from "./routes/ActivityDetail";
import { SettingsPage } from "./routes/Settings";
import { PluginsPage } from "./routes/Plugins";
import { PluginPage } from "./routes/PluginPage";
import { UnlockModal } from "./components/UnlockModal";
import { api } from "./lib/tauri";
import { ToastContainer } from "./components/ui/Toast";
import { ConfirmDialogHost } from "./components/ui/ConfirmDialog";
import { useDropImport } from "./hooks/useDropImport";
import { useWatchFolderListener } from "./hooks/useWatchFolderListener";
import { ImportProgressOverlay } from "./components/import/ImportProgressOverlay";
import { FeedbackModal } from "./components/feedback/FeedbackModal";
import { AppShell } from "./components/layout/AppShell";
import "./App.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
    },
  },
});

/**
 * Shown when the vault can't be opened at boot — most often because it lives
 * in a macOS-protected folder (Documents/Desktop/Downloads) and the app lacks
 * Full Disk Access. Recoverable: grant access (or move the vault back) and
 * reopen. Replaces the previous hard crash at startup.
 */
export function VaultErrorScreen({ message }: { message: string }) {
  const protectedFolder = /Documents|Desktop|Downloads/.test(message);
  const [switchError, setSwitchError] = useState<string | null>(null);

  // Point the app at another vault (or an empty folder for a fresh one)
  // without touching the unopenable vault's data, then restart.
  const pickVault = async (expectExisting: boolean) => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      directory: true,
      title: expectExisting
        ? "Open an existing vault folder"
        : "Choose an empty folder for the new vault",
    });
    if (typeof selected !== "string") return;
    try {
      await api.switchVault(selected, expectExisting);
      await api.restartApp();
    } catch (e) {
      setSwitchError(String(e));
    }
  };

  return (
    <div className="h-screen flex items-center justify-center bg-bg p-8">
      <div className="max-w-md text-center space-y-4">
        <h1 className="text-xl font-bold text-ink">Can't open your vault</h1>
        <p className="text-sm text-muted">{message}</p>
        {protectedFolder && (
          <p className="text-sm text-muted">
            The vault is in a protected folder. Grant Syzify{" "}
            <strong>Full Disk Access</strong> in System Settings → Privacy &amp;
            Security, then reopen — or move the vault back to an unprotected
            location.
          </p>
        )}
        <button
          onClick={() => api.restartApp().catch(() => {})}
          className="btn primary mx-auto"
        >
          Reopen
        </button>
        <div className="flex items-center justify-center gap-2">
          <button onClick={() => pickVault(true)} className="btn ghost">
            Open another vault…
          </button>
          <button onClick={() => pickVault(false)} className="btn ghost">
            Create new vault…
          </button>
        </div>
        <p className="text-xs text-muted">
          The current vault's files stay where they are.
        </p>
        {switchError && <p className="text-sm text-red-500">{switchError}</p>}
      </div>
    </div>
  );
}

// Needs the router context (the drop target depends on the current route),
// so it lives inside <BrowserRouter> rather than in AppContent itself.
function DropImportOverlay() {
  const { dragging, kind } = useDropImport();
  if (!dragging) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-accent/15 backdrop-blur-sm pointer-events-none">
      <div className="bg-card rounded-card shadow-2xl px-10 py-8 text-center border-2 border-dashed border-accent">
        <p className="text-lg font-semibold text-accent-2">
          {kind === "photo" ? "Drop photos to add to this activity" : "Drop workout files to import"}
        </p>
        <p className="text-sm text-muted mt-1">
          {kind === "photo" ? "JPG, PNG, WebP, HEIC" : "GPX, FIT, TCX"}
        </p>
      </div>
    </div>
  );
}

function AppContent() {
  const qc = useQueryClient();
  const { pendingFiles, importing: watchImporting, handleImport: watchImport, handleDismiss: watchDismiss } = useWatchFolderListener();
  const { data: encStatus, isLoading } = useQuery({
    queryKey: ["encryptionStatus"],
    queryFn: () => api.getEncryptionStatus(),
  });
  const { data: vaultError } = useQuery({
    queryKey: ["vaultError"],
    queryFn: () => api.getVaultError(),
  });

  // Refresh activities when background geocoding completes
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    import("@tauri-apps/api/event").then(({ listen }) => {
      listen("activities:updated", () => {
        qc.invalidateQueries({ queryKey: ["activities"] });
        qc.invalidateQueries({ queryKey: ["activity"] });
      }).then((fn) => {
        unlisten = fn;
      });
    });
    return () => unlisten?.();
  }, [qc]);

  if (isLoading) {
    // Same markup as the static boot splash in index.html (inline copy of
    // app-icon.svg; the boot-logo pulse keyframes live in index.html's
    // <style>, which stays in <head> after mount) — seamless handoff.
    return (
      <div
        role="status"
        aria-label="Loading"
        className="h-screen flex items-center justify-center bg-bg"
      >
        <svg className="boot-logo" viewBox="0 0 1024 1024" width="72" height="72" aria-hidden="true">
          <rect x="0" y="0" width="1024" height="1024" rx="205" ry="205" fill="#221f1a" />
          <g transform="translate(160,72) scale(22)">
            <path d="M2 32 L10 16 L15 20 L23 8 L30 32 Z" fill="#f2ece0" strokeLinejoin="round" />
            <circle cx="17.6" cy="9.8" r="3.2" fill="#ef6a2c" />
          </g>
        </svg>
      </div>
    );
  }

  if (vaultError) {
    return <VaultErrorScreen message={vaultError} />;
  }

  if (encStatus?.locked) {
    return (
      <UnlockModal
        onUnlocked={() =>
          qc.invalidateQueries({ queryKey: ["encryptionStatus"] })
        }
      />
    );
  }

  return (
    <BrowserRouter>
      <AppShell>
        {pendingFiles.length > 0 && (
          <div className="flex items-center justify-between gap-3 px-4 py-2.5 bg-accent-soft border-b border-border text-sm shrink-0">
            <span className="text-ink">
              Found <strong>{pendingFiles.length}</strong> new workout file{pendingFiles.length !== 1 ? "s" : ""} in watch folders
            </span>
            <div className="flex items-center gap-2">
              <button
                onClick={watchImport}
                disabled={watchImporting}
                className="px-3 py-1 rounded-field bg-accent text-accent-ink text-xs font-medium hover:bg-accent-2 disabled:opacity-50"
              >
                {watchImporting ? "Importing..." : "Import"}
              </button>
              <button
                onClick={watchDismiss}
                className="px-3 py-1 rounded-field border border-border-2 text-muted text-xs font-medium hover:text-ink hover:bg-card-2"
              >
                Dismiss
              </button>
            </div>
          </div>
        )}
        <div className="flex-1 min-h-0">
          <Routes>
            <Route path="/" element={<DashboardPage />} />
            <Route path="/library" element={<Library />} />
            <Route path="/activity/:id" element={<ActivityDetailPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="/plugins" element={<PluginsPage />} />
            <Route path="/plugin/:pluginId" element={<PluginPage />} />
          </Routes>
        </div>
        <ToastContainer />
        <ConfirmDialogHost />
        <ImportProgressOverlay />
        <FeedbackModal />
        <DropImportOverlay />
      </AppShell>
    </BrowserRouter>
  );
}

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <AppContent />
    </QueryClientProvider>
  );
}

export default App;
