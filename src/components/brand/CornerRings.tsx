/** Faint concentric "contour" rings tucked into a card's top-right corner. */
export function CornerRings() {
  return (
    <svg
      className="pointer-events-none absolute top-0 right-0 text-border"
      width="104"
      height="104"
      viewBox="0 0 104 104"
      fill="none"
      aria-hidden="true"
    >
      {[10, 21, 32, 43, 54].map((r) => (
        <circle key={r} cx="86" cy="18" r={r} stroke="currentColor" strokeWidth="1.5" opacity="0.6" />
      ))}
    </svg>
  );
}
