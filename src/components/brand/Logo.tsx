/**
 * Syzify logo — ascending route line that peaks like a mountain ridge /
 * elevation profile, with a solid "boulder" dot near the summit (the
 * Sisyphus motif). Faithful interpretation of the brand direction; not a
 * final shipped mark. Colors come from theme tokens so it adapts to dark.
 */
interface LogoProps {
  size?: number;
  /** Show the "Syzify" wordmark next to the mark. */
  wordmark?: boolean;
  className?: string;
}

export function Logo({ size = 22, wordmark = true, className }: LogoProps) {
  return (
    <span
      className={className}
      style={{ display: "inline-flex", alignItems: "flex-end", gap: size * 0.3 }}
    >
      <svg
        width={size}
        height={size}
        viewBox="0 0 32 32"
        fill="none"
        style={{ display: "block", flexShrink: 0 }}
        aria-hidden="true"
      >
        <path
          d="M2 32 L10 16 L15 20 L23 8 L30 32 Z"
          fill="var(--ink)"
          strokeLinejoin="round"
        />
        <circle cx="17.6" cy="9.8" r="3.2" fill="var(--accent)" />
      </svg>
      {wordmark && (
        <span
          // role="img" makes the aria-label reliable: ARIA forbids naming
          // bare generic spans, and readers that honor that would otherwise
          // skip the label AND the aria-hidden text — a mute logo.
          role="img"
          aria-label="Syzify"
          style={{
            fontFamily: "var(--font-head)",
            fontWeight: 800,
            fontSize: size * 0.82,
            letterSpacing: "-0.02em",
            color: "var(--ink)",
            lineHeight: 1,
            transform: `translateY(${Math.round(size * 0.12)}px)`,
          }}
        >
          {/* The i's dot is the brand's accent boulder: typeset a dotless ı
              (U+0131, present in the Archivo latin subset) and draw the dot
              ourselves — CSS can't color part of a glyph. aria-hidden keeps
              screen readers on the clean label above instead of "Syzıfy".
              Offsets tuned against Archivo 800's real dot on the harness. */}
          <span aria-hidden="true">
            Syz
            <span style={{ position: "relative", display: "inline-block" }}>
              {"ı"}
              <span
                style={{
                  position: "absolute",
                  left: "50%",
                  top: "0.14em",
                  transform: "translateX(-50%)",
                  width: "0.16em",
                  height: "0.16em",
                  borderRadius: "50%",
                  background: "var(--accent)",
                }}
              />
            </span>
            fy
          </span>
        </span>
      )}
    </span>
  );
}
