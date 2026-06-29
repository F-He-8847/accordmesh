import type { LanguagePreferences, ProviderCapabilities } from "./types";

export const LANGUAGE_CODES = [
  "af", "ar", "hy", "az", "be", "bs", "bg", "ca", "zh-Hans", "zh-Hant",
  "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "gl", "de", "el", "he",
  "hi", "hu", "is", "id", "it", "ja", "kn", "kk", "ko", "lv", "lt", "mk",
  "ms", "mr", "mi", "ne", "no", "fa", "pl", "pt", "ro", "ru", "sr", "sk",
  "sl", "es", "sw", "sv", "tl", "ta", "th", "tr", "uk", "ur", "vi", "cy",
] as const;

export type LanguageCode = (typeof LANGUAGE_CODES)[number];
export type InactivityLockMinutes = 5 | 15 | 30 | 60 | null;

export const NO_TRANSLATION = "none";
export const SAME_AS_TRANSLATION = "same_as_translation";
export const SAME_AS_ANALYSIS = "same_as_analysis";

export const DEFAULT_LANGUAGE_PREFERENCES: LanguagePreferences = {
  uiLocale: "en",
  sourceLanguageMode: "auto",
  sourceLanguage: "en",
  translationTargetLanguage: "en",
  analysisOutputLanguage: "en",
  minutesOutputLanguage: "en",
};

const languageSet = new Set<string>(LANGUAGE_CODES);

export function isLanguageCode(value: unknown): value is LanguageCode {
  return typeof value === "string" && languageSet.has(value);
}

export function languageLabelKey(code: string): string {
  return `languages.options.${code}`;
}

export function normalizeLanguagePreferences(value: unknown): LanguagePreferences {
  const candidate = value && typeof value === "object" ? value as Record<string, unknown> : {};
  const sourceLanguageMode = candidate.sourceLanguageMode === "specified" ? "specified" : "auto";
  const sourceLanguage = isLanguageCode(candidate.sourceLanguage) ? candidate.sourceLanguage : "en";
  const translationTargetLanguage = candidate.translationTargetLanguage === NO_TRANSLATION || isLanguageCode(candidate.translationTargetLanguage)
    ? candidate.translationTargetLanguage
    : "en";
  const analysisOutputLanguage = candidate.analysisOutputLanguage === SAME_AS_TRANSLATION || isLanguageCode(candidate.analysisOutputLanguage)
    ? candidate.analysisOutputLanguage
    : "en";
  const minutesOutputLanguage = candidate.minutesOutputLanguage === SAME_AS_ANALYSIS || isLanguageCode(candidate.minutesOutputLanguage)
    ? candidate.minutesOutputLanguage
    : "en";

  return {
    uiLocale: "en",
    sourceLanguageMode,
    sourceLanguage,
    translationTargetLanguage,
    analysisOutputLanguage,
    minutesOutputLanguage,
  };
}

export function resolveLanguagePreferences(preferences: LanguagePreferences) {
  const normalized = normalizeLanguagePreferences(preferences);
  const translationTargetLanguage = normalized.translationTargetLanguage === NO_TRANSLATION
    ? undefined
    : normalized.translationTargetLanguage;
  const analysisOutputLanguage = normalized.analysisOutputLanguage === SAME_AS_TRANSLATION
    ? translationTargetLanguage ?? normalized.uiLocale
    : normalized.analysisOutputLanguage;
  const minutesOutputLanguage = normalized.minutesOutputLanguage === SAME_AS_ANALYSIS
    ? analysisOutputLanguage
    : normalized.minutesOutputLanguage;
  const sourceLanguage = normalized.sourceLanguageMode === "specified"
    ? normalized.sourceLanguage ?? "en"
    : undefined;

  return {
    sourceLanguage,
    translationTargetLanguage,
    analysisOutputLanguage,
    minutesOutputLanguage,
  };
}

export function normalizeInactivityLockMinutes(value: unknown): InactivityLockMinutes {
  if (value === null || value === "never" || value === 0 || value === "0") return null;
  const numeric = Number(value);
  return numeric === 5 || numeric === 15 || numeric === 30 || numeric === 60 ? numeric : 15;
}

export function providerLanguageCompatibility(
  capabilities: ProviderCapabilities | undefined,
  preferences: LanguagePreferences,
): { supported: boolean; errorCode?: "ERR_SOURCE_LANGUAGE_UNSUPPORTED" | "ERR_TARGET_LANGUAGE_UNSUPPORTED" } {
  if (!capabilities) return { supported: false, errorCode: "ERR_TARGET_LANGUAGE_UNSUPPORTED" };
  const normalized = normalizeLanguagePreferences(preferences);
  const resolved = resolveLanguagePreferences(normalized);
  const sourceSupported = normalized.sourceLanguageMode === "auto"
    ? capabilities.supportsLanguageAutoDetection && capabilities.supportedSourceLanguages.includes("auto")
    : Boolean(resolved.sourceLanguage && capabilities.supportedSourceLanguages.includes(resolved.sourceLanguage));
  if (!sourceSupported) return { supported: false, errorCode: "ERR_SOURCE_LANGUAGE_UNSUPPORTED" };
  if (resolved.translationTargetLanguage && (
    !capabilities.textTranslation ||
    !capabilities.supportedTargetLanguages.includes(resolved.translationTargetLanguage)
  )) {
    return { supported: false, errorCode: "ERR_TARGET_LANGUAGE_UNSUPPORTED" };
  }
  if (!capabilities.supportedTargetLanguages.includes(resolved.analysisOutputLanguage) ||
      !capabilities.supportedTargetLanguages.includes(resolved.minutesOutputLanguage)) {
    return { supported: false, errorCode: "ERR_TARGET_LANGUAGE_UNSUPPORTED" };
  }
  return { supported: true };
}
