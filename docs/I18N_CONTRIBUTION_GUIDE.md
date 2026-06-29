# I18n Contribution Guide

English resources under `apps/desktop/src/i18n/locales/en/` are authoritative.

To add a community locale:

1. Copy the `en` folder.
2. Translate values only.
3. Preserve every key and interpolation placeholder.
4. Register the locale in `locale-registry.ts`.
5. Run `pnpm i18n:validate`.

Business logic must not change for a locale contribution.
