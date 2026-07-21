import type { ViewSpec, ViewElement } from "../../lib/types";
import { PluginMap } from "./PluginMap";
import { Select } from "../ui/Select";

interface RenderProps {
  spec: ViewSpec;
  // Interactive state, supplied by a host that runs the action loop.
  values?: Record<string, string>;
  onChange?: (id: string, value: string) => void;
  onAction?: (action: string) => void;
}

// Renders a plugin's declarative ViewSpec using a fixed set of safe primitives.
// Every value is rendered as text (React escapes it) — no raw HTML, so a plugin
// cannot inject markup or scripts. Inputs/buttons are controlled by the host.
export function PluginViewRenderer({ spec, values, onChange, onAction }: RenderProps) {
  return (
    <div className="space-y-2">
      {spec.title && (
        <h3 className="text-sm font-semibold text-ink">{spec.title}</h3>
      )}
      {spec.elements.map((el, i) => (
        <Element key={i} el={el} values={values} onChange={onChange} onAction={onAction} />
      ))}
    </div>
  );
}

function Element({
  el,
  values,
  onChange,
  onAction,
}: {
  el: ViewElement;
  values?: Record<string, string>;
  onChange?: (id: string, value: string) => void;
  onAction?: (action: string) => void;
}) {
  switch (el.type) {
    case "heading":
      return <h4 className="text-xs font-semibold text-ink">{el.text}</h4>;
    case "text":
      return <p className="text-sm text-muted">{el.text}</p>;
    case "stat":
      return (
        <div className="flex items-baseline justify-between">
          <span className="text-xs text-muted">{el.label}</span>
          <span className="text-sm font-semibold text-ink">{el.value}</span>
        </div>
      );
    case "stat_grid":
      return (
        <div className="grid grid-cols-2 gap-2">
          {el.stats.map((s, i) => (
            <div key={i} className="bg-card rounded p-2">
              <div className="text-[11px] text-muted">{s.label}</div>
              <div className="text-sm font-semibold text-ink">{s.value}</div>
            </div>
          ))}
        </div>
      );
    case "table":
      return (
        <table className="w-full text-xs">
          <thead>
            <tr className="text-left text-muted">
              {el.headers.map((h, i) => (
                <th key={i} className="font-medium py-1">{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {el.rows.map((row, ri) => (
              <tr key={ri} className="border-t border-border">
                {row.map((cell, ci) => (
                  <td key={ci} className="py-1 text-ink">{cell}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
    case "divider":
      return <hr className="border-border" />;
    case "input":
      return (
        <label className="block">
          <span className="text-xs text-muted">{el.label}</span>
          <input
            type={el.input_type === "number" ? "number" : "text"}
            value={values?.[el.id] ?? el.value}
            onChange={(e) => onChange?.(el.id, e.target.value)}
            className="mt-0.5 w-full rounded border border-border px-2 py-1 text-sm"
          />
        </label>
      );
    case "select":
      return (
        <label className="block">
          <span className="text-xs text-muted">{el.label}</span>
          <Select
            ariaLabel={el.label}
            className="mt-0.5 w-full"
            value={values?.[el.id] ?? el.value}
            onChange={(v) => onChange?.(el.id, v)}
            options={el.options.map((o) => ({ value: o, label: o }))}
          />
        </label>
      );
    case "button":
      return (
        <button
          onClick={() => onAction?.(el.action)}
          className="px-3 py-1.5 rounded bg-accent text-white text-sm font-medium hover:bg-accent-2"
        >
          {el.label}
        </button>
      );
    case "map":
      return (
        <div className="space-y-1">
          {el.label && <div className="text-xs text-muted">{el.label}</div>}
          <PluginMap points={el.points} />
        </div>
      );
    default:
      // Unknown element type from a newer plugin — ignore rather than crash.
      return null;
  }
}
