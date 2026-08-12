/** Public feedback address, baked into release builds; VITE_CONTACT_EMAIL overrides it. */
export const CONTACT_EMAIL =
  import.meta.env.VITE_CONTACT_EMAIL ?? "syzify@siliconbasement.com";

/** Bug/feature tracker (issues-only contribution model); VITE_GITHUB_URL overrides it. */
export const GITHUB_ISSUES_URL =
  import.meta.env.VITE_GITHUB_URL ?? "https://github.com/Stasyanz/syzify/issues";
