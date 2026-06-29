# AccordMesh Internationalization Architecture

## 1. Official language

The official first release ships with an English UI only.

This does not limit meeting input, translation, analysis output, or meeting-minutes languages.

## 2. Resource organization

```text
apps/desktop/src/i18n/
├── index.ts
├── locale-registry.ts
├── locale-types.ts
├── validation.ts
└── locales/
    └── en/
        ├── common.json
        ├── unlock.json
        ├── library.json
        ├── realtime.json
        ├── upload.json
        ├── project.json
        ├── analysis.json
        ├── comparison.json
        ├── minutes.json
        ├── settings.json
        ├── providers.json
        ├── errors.json
        └── accessibility.json
```

## 3. Hard-coding prohibition

All user-visible UI text must use translation keys.

This includes:

- buttons;
- headings;
- menus;
- placeholders;
- confirmations;
- empty states;
- validation messages;
- errors;
- accessibility labels;
- overlay text;
- provider capability messages.

Rust returns stable error/status codes. The UI maps them to localized messages.

## 4. English baseline contract

English is authoritative. A community locale must match the English key structure and interpolation placeholders.

Example:

```json
{
  "deleteProjectConfirm": "Delete ‘{{projectName}}’? This action cannot be undone."
}
```

Any community locale must keep `{{projectName}}`.

## 5. Language settings separation

```ts
interface LanguagePreferences {
  uiLocale: string;
  sourceLanguageMode: "auto" | "specified";
  sourceLanguage?: string;
  translationTargetLanguage: string;
  analysisOutputLanguage: string;
  minutesOutputLanguage: string;
}
```

## 6. Community contribution path

A contributor should be able to:

1. copy `locales/en`;
2. create a locale folder;
3. translate resource values;
4. register the locale;
5. run validation;
6. submit a pull request.

No business-logic changes should be necessary.

## 7. Validation hooks

Implement lightweight validation utilities/scripts for:

- missing keys;
- extra keys;
- empty values;
- interpolation mismatch;
- invalid locale registration;
- duplicate keys.

The repository includes a validation command that checks resource structure, required keys, and interpolation placeholders. Locale contributions must pass it before review.
