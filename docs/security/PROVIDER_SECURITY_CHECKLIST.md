# Provider Security Checklist

## Secrets

- [ ] API keys are never included in URLs, query strings, logs, panic messages, or user-facing errors.
- [ ] API keys are validated as non-empty before client construction.
- [ ] Provider endpoints do not embed credentials in the authority section.
- [ ] Custom headers that can carry credentials are treated as sensitive in logs and telemetry.

## Endpoint Safety

- [ ] Remote providers require `https://` endpoints.
- [ ] `http://` is only allowed for loopback/local development providers such as Ollama.
- [ ] Configured endpoints do not include query strings or fragments.
- [ ] Provider-specific default endpoints are pinned to the expected host.

## Input Validation

- [ ] Model identifiers are validated before request dispatch.
- [ ] Request temperature and token limits are clamped or rejected on invalid ranges.
- [ ] Empty prompts or empty message payloads are rejected early.
- [ ] Unsupported message roles are rejected early.

## Error Handling

- [ ] Upstream response bodies are not surfaced directly to end users.
- [ ] User-facing errors are normalized by provider and status code.
- [ ] Secret-bearing strings are redacted before inclusion in any error text.
- [ ] Retry logic only operates on classified transient failures.

## Streaming

- [ ] Streaming implementations fail closed on parse errors.
- [ ] Streaming parsers bound memory usage and do not accumulate unbounded buffers.
- [ ] Partial chunk parsing does not leak raw upstream payloads.
- [ ] Unsupported streaming modes return explicit errors rather than mock content.

## Concurrency

- [ ] Shared state is not held behind async locks across `.await`.
- [ ] Test doubles do not hold mutexes for the lifetime of streams.
- [ ] Global clients are immutable or internally synchronized.

## Dependencies

- [ ] `cargo audit` is run against the current `Cargo.lock`.
- [ ] RustSec warnings are triaged into exploitable vs. maintenance risk.
- [ ] Unmaintained dependencies are tracked with owners and replacement plans.

## Release Gate

- [ ] Security-sensitive provider changes include regression tests.
- [ ] The security assumptions document is updated with each provider addition or protocol change.
- [ ] The audit report is refreshed before shipping provider architecture changes.
