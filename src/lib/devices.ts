// Recording-device registry: turns the `source_device` string a parser stored
// into something showable — a short model label ("fenix 6X", "Edge 840"), a
// full name for the tooltip ("Garmin fenix 6X") and a form factor that picks
// the silhouette. Pure functions over the stored string; nothing here talks to
// the backend, and an unknown string degrades to itself with the generic
// silhouette rather than failing.
//
// What the parsers store (see `compose_device_name` in parser/fit.rs):
//   FIT  → "Garmin fenix6x", "Garmin edge_830", "Garmin fr920xt",
//          "Garmin vivoactive4_small", "Garmin 1620" (id the SDK cannot name);
//          other vendors set `product_name` ("ELEMNT BOLT") or fall back to
//          "Wahoo_fitness 31" (manufacturer enum + numeric product).
//   TCX  → the creator <Name> ("Garmin FR265", "Garmin fenix 7").
//   GPX  → the creator attribute ("StravaGPX", "Garmin Connect").
// The Garmin keys are the FIT SDK's `garmin_product` enum names, so the
// Garmin branch below is a grammar over that enum, not a 400-row table: family
// prefix → family, the remainder → model tokens ("6x" → "6X", "pro_solar" →
// "Pro Solar", regional suffixes dropped).

/** Which silhouette to draw. The model is told apart by the label, not the drawing. */
export type DeviceForm =
  | "watch_multi" // rugged round watch, five buttons: fenix, epix, Enduro, MARQ, tactix, Descent
  | "watch_round" // plain round watch: Forerunner, Venu, vivoactive, Suunto, COROS, Polar
  | "watch_instinct" // round watch with the sub-dial window: Instinct
  | "watch_rect" // rectangular watch or band: Venu Sq, vivosmart, Apple Watch
  | "bike_computer" // head unit with bottom buttons: Edge, ELEMNT, Karoo
  | "generic"; // anything else: sensors, handhelds, apps, unknown

export interface DeviceInfo {
  /** Short model label for the chip: "fenix 6X", "Edge 840", "ELEMNT BOLT". */
  label: string;
  /** Full name for the tooltip: "Garmin fenix 6X". */
  full_name: string;
  form: DeviceForm;
}

/** Suffixes the SDK appends for regional/OEM SKUs of the same hardware. */
const GARMIN_DROP_TOKENS = new Set([
  "asia", "apac", "japan", "jpn", "china", "chn", "taiwan", "twn", "korea", "kor",
  "sea", "thai", "russia", "hebrew", "emea", "ww", "daimler", "bontrager", "4t",
  "single", "byte", "product", "id",
  // Marketing variants that do not change the model name ("sport" is the
  // fenix base trim and is dropped per family, not here: vivomove Sport).
  "large", "oled", "l", "w", "t",
]);

/** Standalone model tokens with a fixed spelling. */
const GARMIN_WORDS: Record<string, string> = {
  plus: "Plus", pro: "Pro", solar: "Solar", music: "Music", hr: "HR", lte: "LTE",
  sq: "Sq", gps: "GPS", jr: "Jr.", premium: "Premium", trend: "Trend",
  titanium: "Titanium", chronos: "Chronos", sapphire: "Sapphire", touring: "Touring",
  crossover: "Crossover", esports: "Esports", remote: "Remote", explore: "Explore",
  air: "Air", ultra: "Ultra", nfc: "NFC", lite: "Lite",
};

/** Letters glued to a number: "6x" → "6X", "645m" → "645 Music". */
const GARMIN_NUMBER_SUFFIX: Record<string, string> = {
  s: "S", x: "X", xt: "XT", xx: "xx", m: " Music", i: "i", l: "", w: "", t: "",
};

interface GarminFamily {
  /** Matches the start of the lowercased key; group 1 is the model remainder. */
  re: RegExp;
  name: string;
  form: DeviceForm;
  /** Model tokens this family does not spell out. */
  drop?: string[];
  /** Families whose label is built differently from "<name> <model>". */
  kind?: "handheld" | "pedals";
}

// Order matters: the first family whose prefix matches wins.
const GARMIN_FAMILIES: GarminFamily[] = [
  { re: /^(?:fr|forerunner)[_ ]?(.*)$/, name: "Forerunner", form: "watch_round" },
  { re: /^fenix[_ ]?(.*)$/, name: "fenix", form: "watch_multi" },
  { re: /^epix[_ ]?(.*)$/, name: "epix", form: "watch_multi" },
  { re: /^enduro[_ ]?(.*)$/, name: "Enduro", form: "watch_multi" },
  { re: /^marq[_ ]?(.*)$/, name: "MARQ", form: "watch_multi" },
  { re: /^tactix[_ ]?(.*)$/, name: "tactix", form: "watch_multi" },
  { re: /^quatix[_ ]?(.*)$/, name: "quatix", form: "watch_multi" },
  { re: /^descent[_ ]?(.*)$/, name: "Descent", form: "watch_multi" },
  { re: /^instinct[_ ]?(.*)$/, name: "Instinct", form: "watch_instinct" },
  { re: /^edge[_ ]?(.*)$/, name: "Edge", form: "bike_computer" },
  { re: /^venu[_ ]?(sq.*)$/, name: "Venu", form: "watch_rect" },
  { re: /^venu[_ ]?(.*)$/, name: "Venu", form: "watch_round" },
  { re: /^vivo[_ ]?active[_ ]?(.*)$/, name: "vivoactive", form: "watch_round" },
  { re: /^vivo[_ ]?move[_ ]?(.*)$/, name: "vivomove", form: "watch_round" },
  { re: /^vivo[_ ]?smart[_ ]?(.*)$/, name: "vivosmart", form: "watch_rect" },
  { re: /^vivo[_ ]?fit[_ ]?(.*)$/, name: "vivofit", form: "watch_rect" },
  { re: /^vivo[_ ]?sport[_ ]?(.*)$/, name: "vivosport", form: "watch_rect" },
  { re: /^vivo[_ ]?ki[_ ]?(.*)$/, name: "vivoki", form: "watch_rect" },
  { re: /^lily[_ ]?(.*)$/, name: "Lily", form: "watch_round" },
  { re: /^swim[_ ]?(.*)$/, name: "Swim", form: "watch_round" },
  { re: /^legacy[_ ]?(.*)$/, name: "Legacy", form: "watch_round" },
  { re: /^d2[_ ]?(.*)$/, name: "D2", form: "watch_round" },
  { re: /^approach[_ ]?(s.*)$/, name: "Approach", form: "watch_round" },
  { re: /^approach[_ ]?(.*)$/, name: "Approach", form: "generic" },
  { re: /^(?:gpsmap|etrex|oregon|rino|foretrex|montana|inreach)[_ ]?(.*)$/, name: "handheld", form: "generic", kind: "handheld" },
  { re: /^virb[_ ]?(.*)$/, name: "VIRB", form: "generic" },
  { re: /^tacx[_ ]?(.*)$/, name: "Tacx", form: "generic" },
  { re: /^(?:vector|rally)[_ ]?(.*)$/, name: "pedals", form: "generic", kind: "pedals" },
  { re: /^varia[_ ]?(.*)$/, name: "Varia", form: "generic" },
  { re: /^hrm[_ ]?(.*)$/, name: "HRM", form: "generic" },
  { re: /^(?:connect|training_center|connectiq_simulator|android_antplus_plugin)$/, name: "Connect", form: "generic" },
];

/** Keys whose enum spelling does not follow the grammar. */
const GARMIN_ODD_KEYS: Record<string, { label: string; form: DeviceForm }> = {
  d2airvenu: { label: "D2 Air", form: "watch_round" },
  venusq: { label: "Venu Sq", form: "watch_rect" },
  venusq2: { label: "Venu Sq 2", form: "watch_rect" },
  venusq2music: { label: "Venu Sq 2 Music", form: "watch_rect" },
  legacy_rey: { label: "Legacy Saga Rey", form: "watch_round" },
  legacy_darth_vader: { label: "Legacy Saga Darth Vader", form: "watch_round" },
  legacy_captain_marvel: { label: "Legacy Hero Captain Marvel", form: "watch_round" },
  legacy_first_avenger: { label: "Legacy Hero First Avenger", form: "watch_round" },
  edge_remote: { label: "Edge Remote", form: "generic" },
  training_center: { label: "Training Center", form: "generic" },
  connectiq_simulator: { label: "Connect IQ simulator", form: "generic" },
  android_antplus_plugin: { label: "ANT+ plugin", form: "generic" },
  connect: { label: "Connect", form: "generic" },
};

const GARMIN_HANDHELD_NAMES: Record<string, string> = {
  gpsmap: "GPSMAP", etrex: "eTrex", oregon: "Oregon", rino: "Rino",
  foretrex: "Foretrex", montana: "Montana", inreach: "inReach",
};

function capitalize(s: string): string {
  return s ? s[0].toUpperCase() + s.slice(1) : s;
}

/** "6x" → "6X", "645m" → "645 Music", "920xt" → "920XT", "255" → "255". */
function numberToken(digits: string, suffix: string): string {
  if (!suffix) return digits;
  const mapped = GARMIN_NUMBER_SUFFIX[suffix];
  if (mapped !== undefined) return digits + mapped;
  return `${digits} ${capitalize(suffix)}`;
}

/** Model tokens of a Garmin key remainder → display words. */
function garminModel(rest: string, drop: string[] = []): string {
  const raw = rest.toLowerCase().split(/[_ ]+/).filter(Boolean);
  const out: string[] = [];
  let afterPro = false;
  for (const tok of raw) {
    if (GARMIN_DROP_TOKENS.has(tok) || drop.includes(tok)) continue;
    if (tok === "small") {
      // "vivoactive4_small" is the 4S; "fr255_small_music" the 255S Music.
      if (out.length) out[out.length - 1] += "S";
      continue;
    }
    let m = /^(\d+)([a-z]*)$/.exec(tok);
    if (m) {
      // A bare number right after "Pro" is a case size (epix Pro 47).
      out.push(afterPro && !m[2] ? `${m[1]} mm` : numberToken(m[1], m[2]));
      afterPro = false;
      continue;
    }
    m = /^([a-z]+)(\d+)([a-z]*)$/.exec(tok);
    if (m) {
      const [, word, num, tail] = m;
      if (word === "gen" || word === "explore" || word === "mach") {
        out.push(`${capitalize(word)} ${num}${tail ? " " + capitalize(tail) : ""}`);
      } else if (word === "mk") {
        out.push(`Mk${num}${tail === "i" ? tail : tail.toUpperCase()}`); // Descent Mk2i, Mk2S
      } else {
        out.push(`${word.toUpperCase()}${num}${tail.toUpperCase()}`); // S62, X10, G12, UT800
      }
      afterPro = false;
      continue;
    }
    afterPro = tok === "pro";
    out.push(GARMIN_WORDS[tok] ?? capitalize(tok));
  }
  return out.join(" ");
}

/** The fenix 6 series is the one where the SDK encodes the trim by absence:
 * `fenix6_sport` / `fenix6S_sport` are the base models, `fenix6` / `fenix6S`
 * the Pro, and `fenix6x` exists only as the 6X Pro. Other generations name
 * their trims outright (`fenix7_pro_solar`, `fenix5s_plus`). */
function fenixSixTrim(model: string): string {
  const m = /^6(S|X)?( Sport)?$/.exec(model);
  if (!m) return model;
  return m[2] ? `6${m[1] ?? ""}` : `6${m[1] ?? ""} Pro`;
}

/** A Garmin key (the part after "Garmin ") → label + form, or null when it is
 * not a recognisable product (a bare product id, an empty string). */
export function garminProduct(key: string): { label: string; form: DeviceForm } | null {
  const k = key.trim().toLowerCase().replace(/ /g, "_");
  if (!k) return null;
  const odd = GARMIN_ODD_KEYS[k];
  if (odd) return odd;
  if (/^\d+$/.test(k)) return null;
  for (const fam of GARMIN_FAMILIES) {
    const m = fam.re.exec(k);
    if (!m) continue;
    if (fam.kind === "handheld") {
      const prefix = /^[a-z]+/.exec(k)?.[0] ?? "";
      const model = garminModel(m[1]);
      return { label: [GARMIN_HANDHELD_NAMES[prefix] ?? capitalize(prefix), model].filter(Boolean).join(" "), form: fam.form };
    }
    if (fam.kind === "pedals") {
      const which = k.startsWith("rally") ? "Rally" : "Vector";
      return { label: [which, garminModel(m[1])].filter(Boolean).join(" "), form: fam.form };
    }
    let model = garminModel(m[1], fam.drop);
    if (fam.name === "fenix") model = fenixSixTrim(model);
    return { label: model ? `${fam.name} ${model}` : fam.name, form: fam.form };
  }
  // Sensors and oddities outside the family grammar: keep the key readable.
  return { label: garminModel(k), form: "generic" };
}

const OTHER_MANUFACTURERS: Record<string, string> = {
  wahoo_fitness: "Wahoo", wahoo: "Wahoo", hammerhead: "Hammerhead", coros: "COROS",
  suunto: "Suunto", polar: "Polar", sigmasport: "SIGMA", bryton: "Bryton",
  stages_cycling: "Stages", zwift: "Zwift", strava: "Strava", peaksware: "TrainingPeaks",
  the_sufferfest: "Wahoo SYSTM", apple: "Apple", samsung: "Samsung", google: "Google",
  fitbit: "Fitbit", xiaomi: "Xiaomi", huawei: "Huawei", amazfit: "Amazfit", igpsport: "iGPSPORT",
  lezyne: "Lezyne", magene: "Magene", cateye: "CatEye", giant_manufacturing_co: "Giant",
  trek: "Trek", specialized: "Specialized", favero_electronics: "Favero", srm: "SRM",
  saris: "Saris", elite: "Elite", kinetic: "Kinetic", development: "Development",
};

/** Recording apps whose creator string implies the hardware. A file made by
 * an app names the app, not the device; where the app runs on one kind of
 * hardware only, the chip can still show the right silhouette and say so. */
const RECORDING_APPS: Record<string, { full_name: string; form: DeviceForm }> = {
  workoutdoors: { full_name: "WorkOutDoors (Apple Watch app)", form: "watch_rect" },
  "apple watch workout": { full_name: "Apple Watch Workout app", form: "watch_rect" },
  athlytic: { full_name: "Athlytic (Apple Watch app)", form: "watch_rect" },
  // Phone apps: the recorder is a phone in a pocket, no watch silhouette.
  stravagpx: { full_name: "Strava app", form: "generic" },
  strava: { full_name: "Strava app", form: "generic" },
  runkeeper: { full_name: "Runkeeper app", form: "generic" },
  komoot: { full_name: "komoot app", form: "generic" },
};

/** Where an app's creator string stops naming the app: "Runkeeper - http://…",
 * "Komoot — iOS", "Strava (Android)". */
const APP_NAME_END = /\s[-–—(]/;

/** Keyword classifier for devices outside the Garmin grammar. */
const FORM_KEYWORDS: Array<[RegExp, DeviceForm]> = [
  [/\belemnt rival\b/i, "watch_round"], // Wahoo's one watch, before its head units
  [/\b(edge|elemnt|bolt|roam|karoo|hammerhead|rox|bryton|rider|igpsport|igs\d|lezyne|dash|dura|mega|xplova|sigma)\b/i, "bike_computer"],
  [/\b(instinct)\b/i, "watch_instinct"],
  [/\b(venu\s*sq|apple watch|fitbit|band|vivosmart|vivofit|sense|versa|charge)\b/i, "watch_rect"],
  [/\b(fenix|f[eē]nix|epix|enduro|marq|tactix|quatix|descent|vertix|vertical|apex|race|peak|grit)\b/i, "watch_multi"],
  // No "garmin" here: real Garmin strings take the grammar above, and a
  // free-text mention ("My Garmin-like tracker") says nothing about shape.
  [/\b(forerunner|fr\d+|venu|vivoactive|vivomove|swim|coros|suunto|polar|pace|vantage|pacer|ignite|watch|lily|amazfit)\b/i, "watch_round"],
];

function classify(text: string): DeviceForm {
  for (const [re, form] of FORM_KEYWORDS) if (re.test(text)) return form;
  return "generic";
}

/** Title-case a FIT manufacturer enum name: "wahoo_fitness" → "Wahoo". */
function manufacturerName(raw: string): string {
  const key = raw.toLowerCase();
  return OTHER_MANUFACTURERS[key] ?? key.split("_").map(capitalize).join(" ");
}

/**
 * Describe a stored `source_device`; null for null/blank input. Never throws:
 * a string the registry does not know comes back as its own label with the
 * silhouette the keyword classifier picks (or the generic one).
 */
export function describeDevice(source: string | null | undefined): DeviceInfo | null {
  const text = source?.trim();
  if (!text) return null;

  const garmin = /^garmin(?:[_ ]+(.*))?$/i.exec(text);
  if (garmin) {
    const rest = garmin[1] ?? "";
    const product = garminProduct(rest);
    if (product) return { label: product.label, full_name: `Garmin ${product.label}`, form: product.form };
    // "Garmin" alone or a product id the SDK could not name.
    const id = rest.trim();
    return {
      label: "Garmin",
      full_name: id ? `Garmin (product ${id})` : "Garmin",
      form: "generic",
    };
  }

  // "<Manufacturer_enum> <numeric product>" — the FIT fallback for a vendor
  // that did not set product_name: the number means nothing to a person.
  // Only an underscored enum name qualifies, because no product is spelled
  // that way; a one-word maker followed by a number is ambiguous ("Suunto 9"
  // is a real watch, "Hammerhead 2" a product id) and is left as it came —
  // a raw label at least matches the file, a made-up one can be wrong.
  const enumProduct = /^([a-z][a-z0-9]*(?:_[a-z0-9]+)+)[ ]+(\d+)$/i.exec(text);
  if (enumProduct) {
    const maker = manufacturerName(enumProduct[1]);
    return { label: maker, full_name: `${maker} (product ${enumProduct[2]})`, form: classify(maker) };
  }

  // A recording app: keep its name as the label, say what it runs on.
  const appName = text.split(APP_NAME_END)[0].trim();
  const app = RECORDING_APPS[appName.toLowerCase()];
  if (app) return { label: clip(appName, MAX_LABEL_CHARS), full_name: app.full_name, form: app.form };

  // A bare manufacturer enum ("Wahoo_fitness") or any free-form creator string.
  const enumOnly = /^([a-z][a-z0-9]*(?:_[a-z0-9]+)+)$/i.exec(text);
  const full = enumOnly ? manufacturerName(enumOnly[1]) : clip(text, MAX_FULL_NAME_CHARS);
  return { label: clip(full, MAX_LABEL_CHARS), full_name: full, form: classify(full) };
}

/** A creator string is free text ("Runkeeper - http://www.runkeeper.com");
 * the chip and its tooltip are not the place for a URL. */
export const MAX_LABEL_CHARS = 32;
const MAX_FULL_NAME_CHARS = 120;

function clip(text: string, max: number): string {
  return text.length > max ? text.slice(0, max - 1).trimEnd() + "…" : text;
}

/** The chip label only — for prose that names the device ("on Edge 840"). */
export function deviceLabel(source: string | null | undefined): string | null {
  return describeDevice(source)?.label ?? null;
}
