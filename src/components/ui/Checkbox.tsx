import { Check } from "lucide-react";

/** Accent checkbox matching the claude.ai/design control style: 15px square,
 * `--border-2` outline when off, `--accent` fill with a white check when on.
 * Renders a real (visually hidden) input, so placing it inside a `<label>`
 * keeps native click/keyboard behavior. */
interface CheckboxProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  ariaLabel?: string;
}

export function Checkbox({ checked, onChange, disabled, ariaLabel }: CheckboxProps) {
  return (
    <span className="relative inline-grid place-items-center shrink-0">
      <input
        type="checkbox"
        className="peer sr-only"
        checked={checked}
        disabled={disabled}
        aria-label={ariaLabel}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span
        className="grid place-items-center w-[15px] h-[15px] rounded-[4px] transition-colors peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-disabled:opacity-40"
        style={{
          border: checked ? "none" : "1.5px solid var(--border-2)",
          background: checked ? "var(--accent)" : "transparent",
          outlineColor: "var(--accent)",
        }}
      >
        {checked && <Check size={11} className="text-white" strokeWidth={3} />}
      </span>
    </span>
  );
}