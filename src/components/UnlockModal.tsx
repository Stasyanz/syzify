import { useState } from "react";
import { Lock, Loader2 } from "lucide-react";
import { api } from "../lib/tauri";
import { Logo } from "./brand/Logo";

interface Props {
  onUnlocked: () => void;
}

export function UnlockModal({ onUnlocked }: Props) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function handleUnlock() {
    if (!password) return;
    setError(null);
    setBusy(true);
    try {
      await api.unlockVault(password);
      onUnlocked();
    } catch (e) {
      setError(`${e}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="fixed inset-0 bg-card flex items-center justify-center z-50">
      <div className="w-full max-w-sm mx-auto p-8 space-y-6">
        <div className="text-center space-y-2">
          <div className="inline-flex items-center justify-center w-16 h-16 rounded-full bg-card-2">
            <Lock size={28} className="text-muted" />
          </div>
          {/* The branded wordmark (accent i-dot included), not plain text —
              this is the most brand-forward screen in the app. */}
          <h1 className="flex justify-center">
            <Logo size={26} />
          </h1>
          <p className="text-sm text-muted">
            Your vault is encrypted. Enter your password to unlock.
          </p>
        </div>

        <div className="space-y-3">
          <input
            type="password"
            placeholder="Password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleUnlock()}
            autoFocus
            className="w-full text-sm border border-border rounded-lg px-4 py-2.5 focus:outline-none focus:ring-2 focus:ring-accent focus:border-transparent"
          />
          {error && (
            <p className="text-xs text-red-500 text-center">{error}</p>
          )}
          <button
            onClick={handleUnlock}
            disabled={busy || !password}
            className="w-full inline-flex items-center justify-center gap-2 text-sm px-4 py-2.5 rounded-lg bg-accent text-accent-ink hover:bg-accent-2 disabled:opacity-50 font-medium"
          >
            {busy && <Loader2 size={15} className="animate-spin" />}
            {busy ? "Unlocking…" : "Unlock"}
          </button>
        </div>
      </div>
    </div>
  );
}
