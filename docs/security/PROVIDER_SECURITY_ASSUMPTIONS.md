# Provider Security Assumptions

Audit date: March 14, 2026

## Trust Boundaries

- Provider API keys are supplied by the local operator through config or environment variables.
- TLS termination for remote providers is trusted when using the provider default endpoint or an explicitly configured `https://` endpoint.
- Loopback `http://` endpoints are only used for local providers such as Ollama and are trusted as local machine traffic.

## Configuration Assumptions

- Users do not intentionally place secrets inside custom endpoint query strings or URL fragments.
- Provider model names come from trusted config or trusted UI selections, not arbitrary untrusted remote input.
- Provider config files on disk are already protected by local filesystem permissions.

## Runtime Assumptions

- `reqwest::Client` remains safe to share across async tasks.
- Provider callers treat returned error strings as user-facing text and do not re-log hidden transport metadata separately.
- Upstream provider streaming payloads may be malformed, truncated, or adversarial and must therefore be parsed defensively.

## Scope Limits

- This audit covered the compiled provider implementation in `crates/rustycode-llm/src/`.
- The checkout used for this audit does not contain `crates/rustycode-llm/src/provider.rs` or `crates/rustycode-llm/src/anthropic_v2.rs`. Any v2 design discussed elsewhere in the repository was not auditable as live code in this tree.
- Dependency results come from `cargo audit` on the workspace lockfile and include non-LLM crates that still affect the shipped binary.

## Residual Risk Assumptions

- Some legacy providers still surface raw upstream error bodies directly. Until those providers are normalized onto the shared error policy, operators should assume provider-side validation messages can still leak internal request details.
- Endpoint validation is only enforced in the remediated providers. Remaining providers should be treated as accepting unsafe custom endpoints until they are migrated.
