import fs from "node:fs";
import path from "node:path";

const root = path.resolve("apps/desktop/src/i18n/locales/en");
const required = [
  "common.json",
  "unlock.json",
  "library.json",
  "realtime.json",
  "upload.json",
  "project.json",
  "analysis.json",
  "comparison.json",
  "minutes.json",
  "settings.json",
  "providers.json",
  "errors.json",
  "accessibility.json",
];

let failures = 0;

for (const file of required) {
  const fullPath = path.join(root, file);
  if (!fs.existsSync(fullPath)) {
    console.error(`Missing ${file}`);
    failures += 1;
    continue;
  }
  const parsed = JSON.parse(fs.readFileSync(fullPath, "utf8"));
  walk(file, parsed);
}

function walk(label, value) {
  if (typeof value === "string") {
    if (!value.trim()) {
      console.error(`Empty value at ${label}`);
      failures += 1;
    }
    return;
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    console.error(`Invalid value at ${label}`);
    failures += 1;
    return;
  }
  for (const [key, child] of Object.entries(value)) {
    walk(`${label}.${key}`, child);
  }
}

if (failures > 0) process.exit(1);
console.log("English i18n resources are valid.");
