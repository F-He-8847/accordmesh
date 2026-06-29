# AccordMesh Data and Security Model

## 1. Threat model and goals

The project should protect against:

- accidental plaintext credential storage;
- sensitive meeting payloads and managed media being exposed through ordinary application files;
- credentials leaking into the frontend or logs;
- access to encrypted content after application lock;
- incomplete deletion leaving usable encrypted data keys;
- cross-project data mixing;
- provider-specific code bypassing security boundaries.

The project cannot protect against a fully compromised operating system, malware running as the user, or disclosure of operational metadata that is not encrypted in the current Developer Preview.

## 2. Local unlock model

First launch:

1. create one local password;
2. derive a key using Argon2id;
3. generate a random master key;
4. encrypt/wrap the master key with the password-derived key;
5. create encrypted provider credential records;
6. initialize project storage.

Later launches show only:

> Enter your local password to unlock

There is no recovery account. Resetting the vault destroys access to the old encrypted data.

## 3. Key hierarchy

```text
User password
  -> Argon2id-derived key
  -> unwrap Master Key
  -> provider credentials
  -> per-project data keys
  -> per-media-file keys
```

Use established authenticated encryption. Do not invent algorithms.

## 4. Credential isolation

- Credentials are read and used only in Rust/native core.
- React receives only masked metadata and provider status.
- Credentials never appear in logs, exported projects, crash reports, or UI state serialization.
- Provider adapters request credentials through the vault service.
- Locking clears sensitive in-memory buffers where practical.

## 5. Sensitive meeting data

- Uploaded media is copied into managed local storage and encrypted.
- Real-time plaintext audio is kept only for the shortest local conversion window. Finalized segments and Stop-time remainders are encrypted into a durable local spool before retryable background processing.
- Encrypted spool chunks are deleted after the required transcript and derived outputs are durably committed.
- Final transcript segments, generated artifacts, uploaded media, and sensitive provider configuration payloads are encrypted at rest.
- Temporary plaintext media files are removed after encryption, successful import, cancellation cleanup, or failure cleanup.

## 6. Operational metadata

The current Developer Preview uses local SQLite records for operational metadata and relationships. These records are local but are not fully encrypted. They may include:

- project titles and original imported file names;
- timestamps, statuses, job and artifact identifiers;
- provider and model identifiers;
- local managed-storage references;
- database relationships and lifecycle state.

Users should choose non-sensitive project titles and file names when metadata disclosure would be a concern. Encrypting or further minimizing operational metadata is a future hardening area.

## 7. Project separation

Each project has a separate project data key. Large media may use independent file keys wrapped by the project or master key.

This supports cryptographic deletion: deleting wrapped keys makes remaining ciphertext unusable even if physical SSD blocks cannot be guaranteed overwritten.

## 8. Auto-lock

Support:

- manual lock;
- inactivity lock;
- background/resume locking after the configured inactivity threshold;
- provider session closure on lock;
- in-memory key clearing;
- UI returning to the unlock screen.

Do not lock silently during an active meeting without warning. Active meeting behavior must be handled explicitly.

## 9. Logging policy

Never log:

- password;
- derived keys;
- master key;
- provider credentials;
- full transcript;
- full translation;
- full analysis;
- raw media.

Allowed logs:

- timestamp;
- module;
- redacted project ID;
- job ID;
- provider ID;
- model ID if non-sensitive;
- error code;
- status code;
- duration;
- retry count.

## 10. Export boundary

Exported plaintext files are outside the encrypted vault. Show a clear warning before export.

A future encrypted archive may be added, but plaintext Markdown/TXT/JSON export is sufficient for the first version.

## 11. Privacy notice

Before real-time assistance starts, display a concise notice that audio is processed and generated text/analysis is stored locally, and that users must follow applicable law and workplace rules.

The app must visibly show that assistance is active. No covert background mode.
