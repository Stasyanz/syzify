/** Public feedback address, baked into release builds; VITE_CONTACT_EMAIL overrides it. */
export const CONTACT_EMAIL =
  import.meta.env.VITE_CONTACT_EMAIL ?? "syzify@siliconbasement.com";
