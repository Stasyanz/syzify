/** Accent toggle switch matching the claude.ai/design settings screen. */
interface ToggleProps {
  on: boolean;
  onToggle: () => void;
  ariaLabel?: string;
}

export function Toggle({ on, onToggle, ariaLabel }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={ariaLabel}
      onClick={onToggle}
      className="w-11 h-[26px] rounded-full border-0 cursor-pointer p-0.5 shrink-0 flex items-center transition-colors"
      style={{
        background: on ? "var(--accent)" : "var(--border-2)",
        justifyContent: on ? "flex-end" : "flex-start",
      }}
    >
      <span className="block w-[22px] h-[22px] rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.25)] transition-all" />
    </button>
  );
}
