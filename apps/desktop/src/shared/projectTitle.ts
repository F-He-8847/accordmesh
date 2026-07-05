export const PROJECT_TITLE_MAX_CHARS = 120;

export function projectTitleLength(value: string): number {
  return Array.from(value).length;
}
