/** Horizontal placement of a chart tooltip whose `left` is the anchor `x` px
 * inside a box `width` px wide. Centred on the anchor while it fits; near
 * either edge the clamp pins it to that edge instead of letting the card cut
 * half the text off (#206). No side switch, so no jump as the cursor crosses
 * the middle, and a tip wider than half the box still fits there. A tip wider
 * than the whole box sits flush left — CSS clamp() lets MIN win. */
export function tipTransform(x: number, width: number): string {
  return `translateX(clamp(${-x}px, -50%, calc(${width - x}px - 100%)))`;
}
