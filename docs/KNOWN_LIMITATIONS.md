# Known Limitations

AccordMesh is a Developer Preview. Current limitations include:

- macOS is the primary validated platform. There is no complete validated Windows or Linux release.
- Windows system-audio loopback capture is not yet implemented.
- The source release is not accompanied by a signed or notarized macOS installer.
- FFmpeg and FFprobe must be installed separately.
- Live OpenAI API smoke testing is pending maintainer API access. Offline provider contract tests do not prove current model availability or real-network behavior.
- Provider quality, language coverage, latency, pricing, and retention vary. Users must review the selected provider's terms.
- External-provider requests necessarily send selected audio or text outside the local device.
- AI output can omit, distort, or invent information. It must not be treated as authoritative evidence without checking the source transcript and recording.
- The first public version does not perform named-speaker diarization or voice identification.
- Translation is text only; there is no translated speech output or audio playback feature.
- macOS system-audio capture may include sounds from other applications played by the Mac.
- Transcript and artifact editing is intentionally read-only; regeneration creates a new immutable version.
- Operational metadata such as project titles, original file names, timestamps, status values, provider/model identifiers, local storage references, and database relationships is stored locally in SQLite and is not fully encrypted. Use non-sensitive titles and file names when necessary.
- Plaintext exports are outside the vault and are not deleted by Reset Vault.
- There is no cloud account, synchronization, password recovery, automatic update service, or enterprise administration.
- Full-text transcript search and advanced evaluation tooling are not yet implemented.
