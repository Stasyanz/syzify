import type { en } from "./en";

/** A message with a form per number, keyed by `Intl.PluralRules`
 * categories. Russian needs one/few/many; `other` is the fallback. */
export type PluralMessage = Partial<Record<Intl.LDMLPluralRule, string>> & { other: string };

export type MessageKey = keyof typeof en;

/** A translation: every English key, a plain string where English has one
 * and plural forms where English has them. */
export type Messages = {
  [K in MessageKey]: (typeof en)[K] extends string ? string : PluralMessage;
};
