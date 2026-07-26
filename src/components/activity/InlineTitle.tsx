import { useEffect, useRef, useState } from "react";
import { MAX_TITLE_LENGTH } from "../../lib/types";

interface Props {
  /** Current display title (custom title or the sport-based fallback). */
  title: string;
  /** Called with the trimmed new title; not called when unchanged/empty. */
  onSave: (title: string) => void;
}

/**
 * The detail-page activity title with in-place editing: click to turn the
 * heading into an input; Enter/blur saves, Escape cancels. An empty draft
 * cancels too — update_activity can't clear a title (None = "don't touch"),
 * and a blank heading would leave nothing to click.
 *
 * The length cap is enforced in CODE (clamp on change AND on save), not via
 * the maxLength attribute — that only limits keystrokes and lets an already
 * overlong value (pre-existing title, paste in some engines) through.
 */
export function InlineTitle({ title, onSave }: Props) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(title);
  // Transient "can't type further" flash when input hits the cap.
  const [limitHit, setLimitHit] = useState(false);
  const limitTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const inputRef = useRef<HTMLInputElement>(null);
  // Blur fires after Enter/Escape already settled — skip its save.
  const settled = useRef(false);

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);
  useEffect(() => () => clearTimeout(limitTimer.current), []);

  const start = () => {
    setDraft(title.slice(0, MAX_TITLE_LENGTH));
    settled.current = false;
    setLimitHit(false);
    setEditing(true);
  };
  const save = () => {
    setEditing(false);
    const trimmed = draft.trim().slice(0, MAX_TITLE_LENGTH);
    if (trimmed && trimmed !== title) onSave(trimmed);
  };
  const cancel = () => setEditing(false);

  const headingClass =
    "text-[21px] font-extrabold tracking-tight text-ink leading-tight";
  const headingStyle = { fontFamily: "var(--font-head)" } as const;

  if (editing) {
    return (
      // Auto-sizing input: an invisible ghost span with the SAME font sits
      // in the same grid cell and defines the width from the draft text;
      // the input just fills the cell. The accent underline then hugs the
      // text instead of stretching across the whole header.
      // minmax(0,auto): without it the grid track refuses to shrink below
      // the ghost's content width and a long draft blows through max-w-full
      // across the whole header.
      <span className="relative inline-grid grid-cols-[minmax(0,auto)] max-w-full align-top">
        <span
          aria-hidden
          className={`${headingClass} invisible whitespace-pre col-start-1 row-start-1 pr-1`}
          style={headingStyle}
        >
          {draft || " "}
        </span>
        <input
          ref={inputRef}
          value={draft}
          onChange={(e) => {
            const v = e.target.value;
            if (v.length > MAX_TITLE_LENGTH) {
              setDraft(v.slice(0, MAX_TITLE_LENGTH));
              setLimitHit(true);
              clearTimeout(limitTimer.current);
              limitTimer.current = setTimeout(() => setLimitHit(false), 1500);
            } else {
              setDraft(v);
            }
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              settled.current = true;
              save();
            } else if (e.key === "Escape") {
              // Keep the page-level Escape (back to library) out of it.
              e.stopPropagation();
              settled.current = true;
              cancel();
            }
          }}
          onBlur={() => {
            if (!settled.current) save();
          }}
          aria-label="Activity title"
          className={`${headingClass} col-start-1 row-start-1 w-full min-w-0 pr-1 bg-transparent outline-none border-b ${
            limitHit ? "border-red-500" : "border-accent"
          }`}
          style={headingStyle}
        />
        {limitHit && (
          // Floating chip below the input: opaque + elevated, so it OVERLAYS
          // the date/meta row like a tooltip instead of mixing with it.
          <span
            role="status"
            className="absolute right-0 top-full mt-1.5 z-10 rounded-md bg-red-700 text-white text-xs font-medium px-2 py-0.5 whitespace-nowrap shadow-md"
          >
            Max {MAX_TITLE_LENGTH} characters
          </span>
        )}
      </span>
    );
  }

  return (
    <h1
      onClick={start}
      data-tip="Click to rename"
      className={`${headingClass} truncate cursor-text`}
      style={headingStyle}
    >
      {title}
    </h1>
  );
}