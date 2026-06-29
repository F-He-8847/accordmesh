export type LocaleCode = "en";
export interface TranslationTree {
  [key: string]: string | TranslationTree;
}
