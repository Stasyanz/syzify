// Sport identity hues — "Trailhead" warm-tuned palette. Sports are grouped
// into hue FAMILIES (all water = teal/cyan/blue region), but every sport gets
// its OWN shade: same-family sports land next to each other on the dashboard
// donut, and identical colors made e.g. Paddling and Open Water merge.
//
// Shades were validated with the dataviz palette checker (OKLCH lightness
// band, chroma floor, Machado CVD separation vs the light surface):
//  - each family passes pairwise within itself (donut-adjacent case);
//  - known accepted compromises, all relieved by legends/gaps per chart:
//    run↔hike ≈ 6.7 (legacy pair, unchanged), treadmill↔walk low under
//    deuteranopia, trail_run↔mountaineering distinct only for normal vision,
//    ski_xc/`walk` sit slightly under the 3:1 surface-contrast bar.
// chart.js / Leaflet need concrete strings, so keep them here in TS.
export const SPORT_COLORS: Record<string, string> = {
  // running — terracotta family
  run: "#c2410c",
  trail_run: "#92400e",
  treadmill: "#ea580c",
  // the run on a simulator (#192): the family's darkest, a chestnut below
  // trail_run. The only red-orange left ≥ 7.8 OKLab from every neighbour
  // for normal vision, deuteranopia and protanopia alike; its contrast on
  // the dark card (1.5) matches yoga's, the palette's floor
  virtual_run: "#682b03",
  // cycling — green family
  ride: "#35894f",
  mountain_bike: "#166534",
  // the trainer rides (#189): a sage and a deep teal — the only two greens
  // left that stay ≥ 5.8 OKLab from every neighbour for normal vision and
  // deuteranopia (the run↔hike compromise level); the family is full.
  // Accepted: both are the palette's palest hues after gray (chroma about
  // half of ride's), and under protanopia indoor_ride sits 4.2 from paddle
  // and 4.8 from ride
  indoor_ride: "#72906c",
  virtual_ride: "#005a5a",
  // on foot — amber / burnt orange
  walk: "#ca8a04",
  hike: "#d8521d",
  mountaineering: "#854d0e",
  // water — teal → cyan → sky
  swim: "#0891b2",
  open_water: "#0369a1",
  sailing: "#0284c7",
  paddle: "#0d9488",
  fishing: "#047857",
  // multisport
  triathlon: "#a21caf",
  // gym — purple family
  strength: "#6d28d9",
  cardio: "#a855f7",
  yoga: "#4c1d95",
  // snow — blue family
  ski: "#2563eb",
  ski_xc: "#60a5fa",
  snowboard: "#1e40af",
  // ball sports — rose family (golf keeps its fairway green)
  golf: "#4d7c0f",
  tennis: "#f43f5e",
  soccer: "#be123c",
  basketball: "#9d174d",
  // deliberately gray: "other" carries no identity
  other: "#78716c",
};

export function getSportColor(sport: string): string {
  return SPORT_COLORS[sport] ?? SPORT_COLORS.other;
}
