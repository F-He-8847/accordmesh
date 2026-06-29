# Security Policy

## Reporting a vulnerability

Do not open a public issue, pull request, or discussion for a suspected vulnerability.

Use this repository's private **Report a vulnerability** function. Include the affected version, platform, expected behavior, observed behavior, and the smallest safe reproduction steps.

Do not attach or paste:

- passwords or provider credentials;
- transcripts, translations, analyses, or meeting minutes;
- raw or encrypted meeting media;
- vault files, databases, key material, or exported projects;
- logs containing sensitive reproduction data.

Use fictional data and redact local paths and identifiers. If the private reporting function is temporarily unavailable, wait until it is restored rather than disclosing security-sensitive details publicly.

## Supported versions

During the Developer Preview, security fixes are applied to the latest public source revision only. No long-term support branch or signed production binary is currently offered.

## Security boundary

AccordMesh is designed to reduce accidental plaintext credential storage, ordinary local-file exposure of encrypted meeting payloads and managed media, and provider credential leakage into the UI. It does not protect against a fully compromised operating system, malware running as the user, malicious platform accessibility tools, disclosure through unencrypted operational metadata, or a provider receiving data that the user intentionally submits to it.

Operational metadata such as project titles, original file names, timestamps, statuses, provider/model identifiers, local storage references, and database relationships is stored locally in SQLite and is not fully encrypted in the current Developer Preview. Plaintext exports are also outside the encrypted vault. Users must protect or delete those files and avoid sensitive project titles or file names when necessary.
