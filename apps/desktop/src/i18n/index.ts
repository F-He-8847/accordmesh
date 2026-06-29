import accessibility from "./locales/en/accessibility.json";
import analysis from "./locales/en/analysis.json";
import common from "./locales/en/common.json";
import comparison from "./locales/en/comparison.json";
import errors from "./locales/en/errors.json";
import library from "./locales/en/library.json";
import languages from "./locales/en/languages.json";
import minutes from "./locales/en/minutes.json";
import project from "./locales/en/project.json";
import providers from "./locales/en/providers.json";
import realtime from "./locales/en/realtime.json";
import settings from "./locales/en/settings.json";
import unlock from "./locales/en/unlock.json";
import upload from "./locales/en/upload.json";

const resources = {
  accessibility,
  analysis,
  common,
  comparison,
  errors,
  library,
  languages,
  minutes,
  project,
  providers,
  realtime,
  settings,
  unlock,
  upload,
} as const;

export type ResourceNamespace = keyof typeof resources;

export function t(key: string, values: Record<string, string | number> = {}): string {
  const [namespace, ...path] = key.split(".");
  let current: unknown = resources[namespace as ResourceNamespace];
  for (const part of path) {
    current = typeof current === "object" && current !== null ? (current as Record<string, unknown>)[part] : undefined;
  }
  const template = typeof current === "string" ? current : key;
  return Object.entries(values).reduce((text, [name, value]) => text.replaceAll(`{{${name}}}`, String(value)), template);
}

export { resources };
