import { resources } from "./index";

export function validateEnglishResources(): string[] {
  const problems: string[] = [];
  for (const [namespace, tree] of Object.entries(resources)) {
    walk(namespace, tree, problems);
  }
  return problems;
}

function walk(path: string, value: unknown, problems: string[]) {
  if (typeof value === "string") {
    if (!value.trim()) problems.push(path);
    return;
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    problems.push(path);
    return;
  }
  for (const [key, child] of Object.entries(value)) {
    walk(`${path}.${key}`, child, problems);
  }
}
