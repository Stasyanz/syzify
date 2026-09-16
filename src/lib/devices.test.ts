import { describe, it, expect } from "vitest";
import { describeDevice, deviceLabel, garminProduct } from "./devices";

// The strings below are what the parsers really store: the FIT SDK's
// `garmin_product` enum names behind "Garmin " (see parser/fit.rs
// compose_device_name), TCX creator names, GPX creator attributes.

describe("garminProduct (FIT SDK enum keys)", () => {
  it.each([
    // The rugged multisport line: family word, size letter, trim words.
    // Series 6: "_sport" keys are the base models, the bare ones the Pro,
    // and the 6X was only ever sold as Pro.
    ["fenix6x", "fenix 6X Pro", "watch_multi"],
    ["fenix6x_asia", "fenix 6X Pro", "watch_multi"],
    ["fenix6", "fenix 6 Pro", "watch_multi"],
    ["fenix6S", "fenix 6S Pro", "watch_multi"],
    ["fenix6_sport", "fenix 6", "watch_multi"],
    ["fenix6S_sport", "fenix 6S", "watch_multi"],
    ["fenix6s_sport_asia", "fenix 6S", "watch_multi"],
    ["fenix7", "fenix 7", "watch_multi"],
    ["fenix5x", "fenix 5X", "watch_multi"],
    ["fenix5s_plus_apac", "fenix 5S Plus", "watch_multi"],
    ["fenix7x_pro_solar", "fenix 7X Pro Solar", "watch_multi"],
    ["fenix3_hr_jpn", "fenix 3 HR", "watch_multi"],
    ["fenix", "fenix", "watch_multi"],
    ["epix_gen2_pro_47", "epix Gen 2 Pro 47 mm", "watch_multi"],
    ["enduro2", "Enduro 2", "watch_multi"],
    ["marq_gen2_aviator", "MARQ Gen 2 Aviator", "watch_multi"],
    ["tactix7", "tactix 7", "watch_multi"],
    ["descent_mk2i", "Descent Mk2i", "watch_multi"],
    ["descent_mk2s", "Descent Mk2S", "watch_multi"],
    // Forerunner: "fr" prefix, XT glued, "m"/"music" → Music, small → S.
    ["fr920xt", "Forerunner 920XT", "watch_round"],
    ["fr645m", "Forerunner 645 Music", "watch_round"],
    ["fr255_small_music", "Forerunner 255S Music", "watch_round"],
    ["fr265_large", "Forerunner 265", "watch_round"],
    ["fr945_lte_asia", "Forerunner 945 LTE", "watch_round"],
    ["fr235l_asia", "Forerunner 235", "watch_round"],
    ["fr225_single_byte_product_id", "Forerunner 225", "watch_round"],
    // vivo*: the SDK spells the family both joined and split.
    ["vivoactive4_small", "vivoactive 4S", "watch_round"],
    ["vivo_active4_large_asia", "vivoactive 4", "watch_round"],
    ["vivoactive3m_w", "vivoactive 3 Music", "watch_round"],
    ["vivo_move_sport", "vivomove Sport", "watch_round"],
    ["vivosmart_5", "vivosmart 5", "watch_rect"],
    ["vivo_fit_jr", "vivofit Jr.", "watch_rect"],
    // Venu is round, Venu Sq is not.
    ["venu2_plus", "Venu 2 Plus", "watch_round"],
    ["venu3s", "Venu 3S", "watch_round"],
    ["venusq2music", "Venu Sq 2 Music", "watch_rect"],
    ["instinct_2x", "Instinct 2X", "watch_instinct"],
    ["instinct_crossover", "Instinct Crossover", "watch_instinct"],
    // Edge: with and without the underscore, Explore and Plus trims.
    ["edge_830", "Edge 830", "bike_computer"],
    ["edge1000_thai", "Edge 1000", "bike_computer"],
    ["Edge_130", "Edge 130", "bike_computer"],
    ["edge_1030_plus", "Edge 1030 Plus", "bike_computer"],
    ["edge_explore2", "Edge Explore 2", "bike_computer"],
    ["edge_touring", "Edge Touring", "bike_computer"],
    ["edge_remote", "Edge Remote", "generic"],
    // Golf watches are watches; golf rangefinders and handhelds are not.
    ["approach_s62", "Approach S62", "watch_round"],
    ["approach_g12_asia", "Approach G12", "generic"],
    ["gpsmap66i", "GPSMAP 66i", "generic"],
    ["etrex_touch", "eTrex Touch", "generic"],
    ["oregon7xx", "Oregon 7xx", "generic"],
    ["d2_mach1_pro", "D2 Mach 1 Pro", "watch_round"],
    ["d2airvenu", "D2 Air", "watch_round"],
    ["legacy_darth_vader", "Legacy Saga Darth Vader", "watch_round"],
    // Sensors, trainers, software: readable, generic.
    ["hrm_pro_plus", "HRM Pro Plus", "generic"],
    ["vector_3", "Vector 3", "generic"],
    ["rally_200", "Rally 200", "generic"],
    ["varia_rct715", "Varia RCT715", "generic"],
    ["tacx_neo2_smart", "Tacx NEO2 Smart", "generic"],
    ["connect", "Connect", "generic"],
    ["android_antplus_plugin", "ANT+ plugin", "generic"],
    ["running_dynamics_pod", "Running Dynamics Pod", "generic"],
  ])("%s → %s (%s)", (key, label, form) => {
    expect(garminProduct(key)).toEqual({ label, form });
  });

  it("spells out a number suffix it has no rule for", () => {
    expect(garminProduct("fenix6q")).toEqual({ label: "fenix 6 Q", form: "watch_multi" });
  });

  it("accepts the spaced spelling a TCX creator uses", () => {
    expect(garminProduct("fenix 7 Sapphire Solar")).toEqual({ label: "fenix 7 Sapphire Solar", form: "watch_multi" });
    expect(garminProduct("FR265")).toEqual({ label: "Forerunner 265", form: "watch_round" });
  });

  it("has no name for a bare product id or nothing at all", () => {
    expect(garminProduct("1620")).toBeNull();
    expect(garminProduct("")).toBeNull();
    expect(garminProduct("  ")).toBeNull();
  });
});

describe("describeDevice", () => {
  it("is null without a device", () => {
    expect(describeDevice(null)).toBeNull();
    expect(describeDevice(undefined)).toBeNull();
    expect(describeDevice("   ")).toBeNull();
  });

  it("names a Garmin product with the manufacturer only in the full name", () => {
    expect(describeDevice("Garmin fenix6x")).toEqual({ label: "fenix 6X Pro", full_name: "Garmin fenix 6X Pro", form: "watch_multi" });
    expect(describeDevice("Garmin edge_840")).toEqual({ label: "Edge 840", full_name: "Garmin Edge 840", form: "bike_computer" });
    expect(describeDevice("garmin FR265")).toEqual({ label: "Forerunner 265", full_name: "Garmin Forerunner 265", form: "watch_round" });
  });

  it("keeps a Garmin product id the SDK could not name out of the label", () => {
    expect(describeDevice("Garmin 1620")).toEqual({ label: "Garmin", full_name: "Garmin (product 1620)", form: "generic" });
    expect(describeDevice("Garmin")).toEqual({ label: "Garmin", full_name: "Garmin", form: "generic" });
  });

  it("shows a vendor's product_name as-is and classifies it by keyword", () => {
    expect(describeDevice("ELEMNT BOLT")).toEqual({ label: "ELEMNT BOLT", full_name: "ELEMNT BOLT", form: "bike_computer" });
    expect(describeDevice("Karoo 2")).toMatchObject({ form: "bike_computer" });
    expect(describeDevice("COROS VERTIX 2")).toMatchObject({ form: "watch_multi" });
    expect(describeDevice("Suunto 9 Peak")).toMatchObject({ form: "watch_multi" });
    expect(describeDevice("Polar Vantage V2")).toMatchObject({ form: "watch_round" });
    expect(describeDevice("Apple Watch")).toMatchObject({ form: "watch_rect" });
    expect(describeDevice("Zepp Export")).toEqual({ label: "Zepp Export", full_name: "Zepp Export", form: "generic" });
  });

  it("turns an underscored manufacturer enum with a numeric product into the maker's name", () => {
    expect(describeDevice("Wahoo_fitness 31")).toEqual({ label: "Wahoo", full_name: "Wahoo (product 31)", form: "generic" });
    expect(describeDevice("Some_new_vendor 5")).toMatchObject({ label: "Some New Vendor", form: "generic" });
    expect(describeDevice("Wahoo_fitness")).toMatchObject({ label: "Wahoo" });
  });

  it("leaves a one-word maker with a number alone: it may be a real model", () => {
    // "Suunto 9" is a watch and "Hammerhead 2" a product id; the string
    // cannot tell them apart, and a raw label is the cheaper mistake.
    expect(describeDevice("Suunto 9")).toEqual({ label: "Suunto 9", full_name: "Suunto 9", form: "watch_round" });
    expect(describeDevice("Hammerhead 2")).toEqual({ label: "Hammerhead 2", full_name: "Hammerhead 2", form: "bike_computer" });
    expect(describeDevice("Karoo 2")?.label).toBe("Karoo 2");
  });

  it("clips a long creator string for the chip and keeps a URL out of the tooltip", () => {
    const d = describeDevice("Some Tracker - http://www.example.com/a/very/long/path")!;
    expect(d.label).toBe("Some Tracker - http://www.examp…");
    expect(d.label.length).toBeLessThanOrEqual(32);
    expect(d.full_name).toBe("Some Tracker - http://www.example.com/a/very/long/path");
    expect(describeDevice("x".repeat(300))!.full_name.length).toBe(120);
  });

  it("knows the hardware behind a recording app's creator string", () => {
    // WorkOutDoors runs only on Apple Watch: the file names the app, the chip
    // shows the watch and says so on hover.
    expect(describeDevice("WorkOutDoors")).toEqual({ label: "WorkOutDoors", full_name: "WorkOutDoors (Apple Watch app)", form: "watch_rect" });
    // Phone apps get the generic silhouette and a plain label without the URL.
    expect(describeDevice("Runkeeper - http://www.runkeeper.com")).toEqual({ label: "Runkeeper", full_name: "Runkeeper app", form: "generic" });
    expect(describeDevice("StravaGPX")).toEqual({ label: "StravaGPX", full_name: "Strava app", form: "generic" });
    expect(describeDevice("Komoot — iOS")).toMatchObject({ label: "Komoot", full_name: "komoot app" });
    // The FIT fallback "<maker> <product id>" is not an app creator string.
    expect(describeDevice("Strava 123")).toMatchObject({ label: "Strava 123", full_name: "Strava 123" });
  });

  it("does not let a free-text mention of Garmin or a Wahoo watch pick the wrong silhouette", () => {
    expect(describeDevice("My Garmin-like tracker")).toMatchObject({ form: "generic" });
    expect(describeDevice("ELEMNT RIVAL")).toMatchObject({ form: "watch_round" });
    expect(describeDevice("ELEMNT ROAM")).toMatchObject({ form: "bike_computer" });
  });
});

describe("deviceLabel", () => {
  it("is the chip label or null", () => {
    expect(deviceLabel("Garmin edge_840")).toBe("Edge 840");
    expect(deviceLabel(null)).toBeNull();
  });
});
