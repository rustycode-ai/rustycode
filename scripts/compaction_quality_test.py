#!/usr/bin/env python3 -u
"""
Compaction Quality Benchmark
=============================
Compares three compaction approaches on the same conversation:
1. Piggyback — inject tool into next LLM call, get summary as side-effect
2. Server (SummarizeTier) — dedicated LLM call for summarization
3. Client (Truncate) — hard cut to tail turns, no LLM

Measures:
- Token savings (input tokens saved)
- Latency (wall-clock time)
- Quality score (how much key information is preserved)
"""

import json
import os
import sys
import time
import urllib.request
import urllib.error
from dataclasses import dataclass, field

# --- Config ---
with open(os.path.expanduser("~/.rustycode/config.json")) as f:
    config = json.load(f)
openai_cfg = config["providers"]["openai"]
BASE_URL = openai_cfg["base_url"].rstrip("/")
API_KEY = openai_cfg["api_key"]
MODEL = "glm-4.7"  # Most reliable tool caller from validation

# --- Realistic conversation (15 messages, ~3K tokens) ---
CONVERSATION = [
    {"role": "user", "content": "I'm building a Rust TUI app with ratatui for an AI coding assistant. I need help with the event loop architecture and async command handling."},
    {"role": "assistant", "content": "For a ratatui event loop with async, I recommend the MPSC channel pattern with tokio:\n\n```rust\nasync fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> Result<()> {\n    let (tx, mut rx) = tokio::sync::mpsc::channel(100);\n\n    // Spawn event capture task\n    tokio::spawn(async move {\n        loop {\n            let event = capture_event().await;\n            tx.send(Event::from(event)).await.unwrap();\n        }\n    });\n\n    loop {\n        terminal.draw(|f| ui(f))?;\n        tokio::select! {\n            Some(event) = rx.recv() => handle_event(event),\n            _ = tokio::time::sleep(Duration::from_millis(100)) => (),\n        }\n    }\n}\n```\n\nThis separates event capture from rendering and keeps the UI responsive."},
    {"role": "user", "content": "Good. Now add support for streaming LLM responses. The app needs to render tokens as they arrive. Also found a bug where Ctrl+C doesn't restore the terminal on macOS — the cursor stays hidden."},
    {"role": "assistant", "content": "For streaming, use a bounded channel that the LLM provider writes to and the UI reads from:\n\n```rust\n// In src/streaming.rs\npub struct StreamSink {\n    tx: mpsc::Sender<StreamEvent>,\n}\n\nimpl StreamSink {\n    pub async fn feed(&self, chunk: &str) -> Result<()> {\n        self.tx.send(StreamEvent::Token(chunk.to_string())).await\n    }\n}\n```\n\nFor the Ctrl+C cleanup bug in `src/event_loop.rs:47`, you need to restore terminal mode in a signal handler AND a Drop impl:\n\n```rust\npub struct TerminalGuard<'a, B: Backend>(&'a mut Terminal<B>);\n\nimpl<'a, B: Backend> Drop for TerminalGuard<'a, B> {\n    fn drop(&mut self) {\n        let _ = disable_raw_mode();\n        let _ = self.0.backend_mut().execute(LeaveAlternateScreen);\n        let _ = self.0.show_cursor();\n    }\n}\n```\n\nCall this guard at the top of run_app so cleanup happens even on panic."},
    {"role": "user", "content": "I need to implement a security fix — the bash tool passes user input directly to shell commands. File is `crates/rustycode-tools/src/bash.rs:128`. Also the config at `/etc/rustycode/config.toml` needs migration from TOML to the new JSON format, preserving all existing settings."},
    {"role": "assistant", "content": "For the shell injection fix in `bash.rs:128`, use shlex escaping + blocklist validation:\n\n```rust\nfn sanitize_command(input: &str) -> Result<String> {\n    // Block dangerous patterns\n    let dangerous = [\"&&\", \"||\", \";\", \"$(\", \"`\", \">\", \">>\", \"|\"];\n    for pattern in &dangerous {\n        if input.contains(pattern) {\n            return Err(anyhow!(\"Dangerous shell pattern detected: {}\", pattern));\n        }\n    }\n    Ok(shlex::try_quote(input)?.to_string())\n}\n```\n\nFor the config migration at `/etc/rustycode/config.toml`:\n\n```rust\n#[derive(Deserialize)]\nstruct LegacyConfig {\n    #[serde(rename = \"api_endpoint\", alias = \"api_url\", alias = \"endpoint\")]\n    endpoint: String,\n    #[serde(rename = \"model_name\", alias = \"model\")]\n    model: String,\n    max_tokens: Option<usize>,\n    temperature: Option<f32>,\n}\n\nfn migrate_config(toml_path: &Path) -> Result<PathBuf> {\n    let content = fs::read_to_string(toml_path)?;\n    let legacy: LegacyConfig = toml::from_str(&content)?;\n    let new_config = NewConfig::from_legacy(legacy);\n    let json_path = toml_path.with_extension(\"json\");\n    fs::write(&json_path, serde_json::to_string_pretty(&new_config)?)?;\n    Ok(json_path)\n}\n```\n\nNext steps: 1) Add fuzz testing for sanitize_command, 2) Write config migration unit tests, 3) Update bash tool integration tests."},
    {"role": "user", "content": "The streaming works but there's a race condition — when the user sends a new message while streaming is still in progress, the response gets mixed. Also I need to add support for the Anthropic API alongside OpenAI. The provider abstraction is in `crates/rustycode-llm/src/provider.rs`."},
    {"role": "assistant", "content": "For the race condition in streaming, you need a cancellation token:\n\n```rust\n// In src/streaming.rs\npub struct ActiveStream {\n    cancellation: CancellationToken,\n    handle: JoinHandle<()>,\n}\n\nimpl ActiveStream {\n    pub fn cancel(&self) {\n        self.cancellation.cancel();\n    }\n}\n```\n\nWhen a new user message arrives, cancel the active stream before starting a new one:\n\n```rust\nif let Some(active) = &self.active_stream {\n    active.cancel();\n}\nself.active_stream = Some(start_stream(new_message));\n```\n\nFor the Anthropic provider in `provider.rs`, extend the existing trait:\n\n```rust\n#[async_trait]\npub trait LLMProvider: Send + Sync {\n    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;\n    async fn stream(&self, request: CompletionRequest) -> Result<StreamSink>;\n    fn model_name(&self) -> &str;\n    fn max_context_tokens(&self) -> usize { 128_000 }\n}\n\npub struct AnthropicProvider {\n    client: reqwest::Client,\n    api_key: SecretString,\n    model: String,\n}\n```\n\nThe Anthropic API uses a different message format (content blocks instead of plain strings) so you'll need an adapter layer in `crates/rustycode-llm/src/anthropic.rs`."},
    {"role": "user", "content": "Now I need to implement the tool permission system. Users should be able to approve/deny tool calls. The permission logic is in `crates/rustycode-tools/src/security.rs`. I also need to add a sandbox mode that restricts file system access to the project directory only."},
    {"role": "assistant", "content": "For the permission system in `security.rs`, use a capability-based approach:\n\n```rust\n#[derive(Debug, Clone)]\npub enum Permission {\n    Allow,\n    Deny,\n    Ask,  // Prompt user\n}\n\n#[derive(Debug)]\npub struct PermissionManager {\n    rules: Vec<PermissionRule>,\n    session_decisions: HashMap<String, Permission>,\n}\n\nimpl PermissionManager {\n    pub fn check(&self, tool: &str, args: &Value) -> Permission {\n        for rule in &self.rules {\n            if rule.matches(tool, args) {\n                return rule.permission.clone();\n            }\n        }\n        Permission::Ask\n    }\n}\n```\n\nFor sandbox mode, use platform-specific sandboxing:\n\n```rust\n// macOS: Seatbelt (sandbox-exec)\nfn create_sandbox(project_dir: &Path) -> Result<SandboxGuard> {\n    let profile = format!(\n        \"(version 1)(allow file-read*)(allow file-write* (subdirectory \\\"{}\\\"))\",\n        project_dir.display()\n    );\n    SandboxGuard::with_profile(&profile)\n}\n```\n\nThe sandbox is already implemented in `crates/rustycode-sandbox/` — you just need to wire it into the permission manager as a pre-check layer.\n\nRemaining work: 1) Add permission persistence to session state, 2) Wire sandbox into bash tool execution, 3) Add 'always allow' / 'always deny' session memory."},
]

# --- Key facts to check for quality scoring ---
KEY_FACTS = [
    # (fact_id, fact_text_to_check, category, weight)
    ("framework", "ratatui", "tech", 2),
    ("streaming", "streaming", "feature", 1),
    ("ctrlc_bug", "event_loop.rs", "bug", 2),
    ("ctrlc_fix", "disable_raw_mode", "fix", 2),
    ("shell_inject", "bash.rs:128", "security", 2),
    ("sanitize", "shlex", "security", 2),
    ("config_path", "/etc/rustycode/config.toml", "file", 2),
    ("config_migrate", "migration", "task", 1),
    ("race_cond", "race condition", "bug", 2),
    ("cancel_token", "CancellationToken", "fix", 2),
    ("anthropic", "Anthropic", "feature", 1),
    ("provider_file", "provider.rs", "file", 2),
    ("permission", "Permission", "feature", 1),
    ("security_file", "security.rs", "file", 2),
    ("sandbox", "sandbox", "feature", 1),
    ("next_steps", "next steps", "planning", 1),
    ("project_dir", "project directory", "scope", 1),
]

CATEGORIES = {
    "tech": "Technology choices",
    "feature": "Features discussed",
    "bug": "Bugs identified",
    "fix": "Fixes implemented",
    "security": "Security concerns",
    "file": "File paths",
    "task": "Tasks mentioned",
    "planning": "Planning/next steps",
    "scope": "Project scope",
}

# --- Token estimation ---
def estimate_tokens(text: str) -> int:
    return len(text.split())

def estimate_messages_tokens(messages: list) -> int:
    return sum(estimate_tokens(m["content"] or "") for m in messages)

# --- Quality scoring ---
def score_quality(summary_text: str, compacted_messages: list = None) -> dict:
    """Score how well key facts are preserved."""
    check_text = summary_text.lower()
    if compacted_messages:
        for m in compacted_messages:
            check_text += " " + (m.get("content") or "").lower()

    results = {}
    total_weight = 0
    preserved_weight = 0
    by_category = {}

    for fact_id, fact_text, category, weight in KEY_FACTS:
        total_weight += weight
        preserved = fact_text.lower() in check_text
        results[fact_id] = preserved
        if preserved:
            preserved_weight += weight

        if category not in by_category:
            by_category[category] = {"preserved": 0, "total": 0, "facts": []}
        by_category[category]["total"] += weight
        if preserved:
            by_category[category]["preserved"] += weight
        by_category[category]["facts"].append((fact_id, preserved))

    score = preserved_weight / total_weight * 100 if total_weight > 0 else 0
    return {
        "score": round(score, 1),
        "preserved_weight": preserved_weight,
        "total_weight": total_weight,
        "facts": results,
        "by_category": {k: v for k, v in sorted(by_category.items())},
        "missing": [fid for fid, ok in results.items() if not ok],
    }

# --- API call helper ---
def call_api(messages: list, tools: list = None, timeout: int = 60) -> tuple:
    url = f"{BASE_URL}/chat/completions"
    body = {
        "model": MODEL,
        "messages": messages,
        "temperature": 0.3,
        "max_tokens": 2048,
    }
    if tools:
        body["tools"] = tools
        body["tool_choice"] = "auto"

    data = json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, headers={
        "Content-Type": "application/json",
        "Authorization": f"Bearer {API_KEY}",
    })
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read()), None
    except urllib.error.HTTPError as e:
        return None, f"HTTP {e.code}: {e.read().decode()[:200]}"
    except Exception as e:
        return None, str(e)[:200]

# --- Approach 1: Piggyback ---
def run_piggyback() -> dict:
    print("[Piggyback] Starting...", flush=True)
    start = time.time()

    system = (
        "IMPORTANT: You MUST call the compact_context tool in your response alongside "
        "answering the user. This is required to free context space. Answer the user "
        "FIRST, then call compact_context."
    )
    tool_def = {
        "type": "function",
        "function": {
            "name": "compact_context",
            "description": "Condense earlier conversation turns into a summary to free context space.",
            "parameters": {
                "type": "object",
                "properties": {
                    "summary": {
                        "type": "string",
                        "description": "Concise summary capturing key facts, decisions, file paths, errors, and next steps."
                    }
                },
                "required": ["summary"]
            }
        }
    }

    # Add a final user message to trigger the response
    messages = [{"role": "system", "content": system}] + CONVERSATION + [
        {"role": "user", "content": "Great progress! Can you summarize everything we've done so far and what's left?"}
    ]

    input_tokens = estimate_messages_tokens(messages)

    resp, err = call_api(messages, [tool_def], timeout=90)
    latency = time.time() - start

    if err:
        return {"error": err, "latency": latency, "input_tokens": input_tokens}

    choice = resp["choices"][0]["message"]
    text = choice.get("content") or ""
    tool_calls = choice.get("tool_calls") or []

    summary = ""
    called = False
    for tc in tool_calls:
        if tc.get("function", {}).get("name") == "compact_context":
            called = True
            try:
                summary = json.loads(tc["function"]["arguments"]).get("summary", "")
            except:
                summary = "(parse error)"

    output_tokens = estimate_tokens(text) + estimate_tokens(summary)

    quality = score_quality(summary if summary else text)

    return {
        "tool_called": called,
        "summary_chars": len(summary),
        "text_chars": len(text),
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "latency": round(latency, 2),
        "quality": quality,
    }

# --- Approach 2: Server compaction (SummarizeTier) ---
def run_server_compaction() -> dict:
    print("[Server] Starting...", flush=True)
    start = time.time()

    # Full 9-section summary prompt (matches SummarizeTier Full template)
    full_prompt = """You are a conversation summarizer. Summarize the conversation below into a structured summary with these 9 sections. Be concise but thorough.

## Primary Request
What the user originally asked for.

## Key Concepts
Important concepts, terminology, and domain knowledge mentioned.

## Files/Code
Files read, written, or modified. Include paths and key changes.

## Errors/Fixes
Errors encountered and how they were resolved.

## Problem Solving
Approach taken, alternatives considered, and reasoning.

## User Messages
Key user messages or instructions that changed direction.

## Pending Tasks
Tasks mentioned but not yet completed.

## Current Work
What was being worked on most recently.

## Next Step
The logical next step based on where the conversation left off.

---

CONVERSATION:

"""

    conversation_text = "\n\n".join(
        f"[{m['role']}]: {m['content']}" for m in CONVERSATION
    )
    prompt = full_prompt + conversation_text

    messages = [{"role": "user", "content": prompt}]
    input_tokens = estimate_tokens(prompt)

    resp, err = call_api(messages, timeout=90)
    latency = time.time() - start

    if err:
        return {"error": err, "latency": latency, "input_tokens": input_tokens}

    summary = resp["choices"][0]["message"].get("content", "")
    output_tokens = estimate_tokens(summary)

    quality = score_quality(summary)

    return {
        "summary_chars": len(summary),
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "latency": round(latency, 2),
        "quality": quality,
    }

# --- Approach 3: Client compaction (Truncate) ---
def run_client_compaction(tail_turns: int = 2) -> dict:
    print(f"[Client/Truncate tail={tail_turns}] Starting...", flush=True)
    start = time.time()

    all_tokens = estimate_messages_tokens(CONVERSATION)

    # Find user message boundaries
    user_indices = [i for i, m in enumerate(CONVERSATION) if m["role"] == "user"]
    num_turns = len(user_indices)

    if num_turns <= tail_turns:
        kept = CONVERSATION
    else:
        split_at = user_indices[num_turns - tail_turns]
        kept = CONVERSATION[split_at:]

    kept_tokens = estimate_messages_tokens(kept)
    removed_tokens = all_tokens - kept_tokens
    latency = time.time() - start

    quality = score_quality("", kept)

    return {
        "tail_turns": tail_turns,
        "messages_kept": len(kept),
        "messages_removed": len(CONVERSATION) - len(kept),
        "input_tokens": all_tokens,
        "tokens_after": kept_tokens,
        "tokens_removed": removed_tokens,
        "latency": round(latency, 4),
        "quality": quality,
    }

# --- Main ---
print("=" * 70)
print("COMPACTION QUALITY BENCHMARK")
print(f"Model: {MODEL}  |  Conversation: {len(CONVERSATION)} messages  |  "
      f"~{estimate_messages_tokens(CONVERSATION)} tokens")
print(f"Key facts: {len(KEY_FACTS)} (total weight: {sum(w for _,_,_,w in KEY_FACTS)})")
print("=" * 70)

# Run all approaches
results = {}

print()
results["piggyback"] = run_piggyback()
time.sleep(2)

results["server"] = run_server_compaction()
time.sleep(2)

results["client_2turn"] = run_client_compaction(tail_turns=2)
results["client_1turn"] = run_client_compaction(tail_turns=1)

# --- Print results ---
print("\n" + "=" * 70)
print("RESULTS")
print("=" * 70)

# Summary table
print(f"\n{'Approach':<20} {'Quality':>8} {'Input TOK':>10} {'Output':>8} {'Latency':>8} {'Notes'}")
print("-" * 80)

for name, r in results.items():
    if "error" in r:
        print(f"{name:<20} {'ERROR':>8} {r.get('input_tokens',0):>10} {'-':>8} {r['latency']:>7.2f}s {r['error'][:40]}")
        continue

    q = r["quality"]["score"]
    inp = r.get("input_tokens", r.get("input_tokens", 0))
    out = r.get("output_tokens", r.get("tokens_after", 0))
    lat = r["latency"]

    notes = ""
    if name == "piggyback":
        notes = f"tool_called={r['tool_called']}" if "tool_called" in r else ""
    elif name.startswith("client"):
        notes = f"removed={r['tokens_removed']}tok"

    print(f"{name:<20} {q:>7.1f}% {inp:>10} {out:>8} {lat:>7.2f}s {notes}")

# Detailed quality breakdown
print(f"\n{'KEY FACT PRESERVATION':<20} ", end="")
print("  ".join(f"{name:<14}" for name in results.keys()))
print("-" * 80)

for fact_id, fact_text, category, weight in KEY_FACTS:
    row = f"{fact_id:<20} "
    for name, r in results.items():
        if "error" in r:
            row += "ERR           "
        elif r["quality"]["facts"].get(fact_id, False):
            row += f"{'PRESERVED':<14}"
        else:
            row += f"{'MISSING':<14}"
    print(row)

# Category scores
print(f"\n{'CATEGORY SCORES':<20} ", end="")
print("  ".join(f"{name:<14}" for name in results.keys()))
print("-" * 80)
all_categories = set()
for r in results.values():
    if "quality" in r and "by_category" in r["quality"]:
        all_categories.update(r["quality"]["by_category"].keys())

for cat in sorted(all_categories):
    cat_name = CATEGORIES.get(cat, cat)
    row = f"{cat_name:<20} "
    for name, r in results.items():
        if "error" in r or "by_category" not in r.get("quality", {}):
            row += "-             "
        else:
            c = r["quality"]["by_category"].get(cat, {"preserved": 0, "total": 1})
            pct = c["preserved"] / c["total"] * 100 if c["total"] > 0 else 0
            row += f"{pct:>5.0f}% ({c['preserved']}/{c['total']:<3})"
    print(row)

# Save results
os.makedirs(".reports/compaction", exist_ok=True)
with open(".reports/compaction/quality_benchmark.json", "w") as f:
    json.dump({
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"),
        "model": MODEL,
        "conversation_messages": len(CONVERSATION),
        "conversation_tokens": estimate_messages_tokens(CONVERSATION),
        "key_facts": len(KEY_FACTS),
        "results": results,
    }, f, indent=2, default=str)
print(f"\nReport saved to .reports/compaction/quality_benchmark.json")
