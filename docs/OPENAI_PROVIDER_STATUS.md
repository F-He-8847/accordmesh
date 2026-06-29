# OpenAI Provider Status

The repository includes an OpenAI provider adapter behind AccordMesh's provider-neutral interfaces. It implements request construction, credential isolation, error mapping, cancellation boundaries, structured-output validation, transcription flow, and model override validation.

Offline tests and fictional local HTTP fixtures validate these contracts without using a real API key.

As of the Developer Preview source snapshot, live OpenAI API smoke testing remains pending maintainer API access. The project therefore does not claim that every configured model name is currently available, that every schema is accepted by every model, or that real-network cancellation and latency match offline fixtures.

When live testing is performed, it must use fictional short text and audio, a dedicated restricted project key, a small budget, and result packages that exclude credentials and sensitive content.
