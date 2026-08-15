/** Public feedback address, baked into release builds; VITE_CONTACT_EMAIL overrides it. */
export const CONTACT_EMAIL =
  import.meta.env.VITE_CONTACT_EMAIL || "syzify@siliconbasement.com";

const githubUrlOverride = import.meta.env.VITE_GITHUB_URL;
// Only accept an https:// override: a hostile build env could otherwise inject a
// javascript:/data: href that the non-http-link interceptor would let through.
export const GITHUB_ISSUES_URL =
  githubUrlOverride && githubUrlOverride.startsWith("https://")
    ? githubUrlOverride
    : "https://github.com/Stasyanz/syzify/issues";
