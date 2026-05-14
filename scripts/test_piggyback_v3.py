#!/usr/bin/env python3 -u
"""Quick piggyback validation: test explicit + strong prompts across z.ai models."""
import json, os, sys, time, urllib.request, urllib.error

# Load config
with open(os.path.expanduser("~/.rustycode/config.json")) as f:
    config = json.load(f)

openai_cfg = config["providers"]["openai"]
BASE_URL = openai_cfg["base_url"].rstrip("/")
API_KEY = openai_cfg["api_key"]
MODELS = openai_cfg.get("models", ["glm-5.1", "glm-4.7", "glm-4.5-air"])

NORMAL_SUFFIX = (
    "[Context Management] The conversation has grown long. A compact_context tool "
    "has been added to your available tools. Please call it alongside your response "
    "to this message so that older turns can be condensed. Your response to the "
    "user is the priority — the tool call is a secondary side-effect."
)
STRONG_SUFFIX = (
    "IMPORTANT: You MUST call the compact_context tool in your response alongside "
    "answering the user. This is required to free context space. Answer the user "
    "FIRST, then call compact_context."
)

TOOL_DEF = {
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

# Realistic multi-turn conversation
CONVERSATION = [
    {"role": "user", "content": "I'm building a Rust TUI app with ratatui. I need help with the event loop architecture."},
    {"role": "assistant", "content": "For a ratatui event loop, I recommend the MPSC channel pattern:\n\n```rust\nfn run_app<B: Backend>(terminal: &mut Terminal<B>, tx: mpsc::Sender<Event>) -> Result<()> {\n    loop {\n        terminal.draw(|f| ui(f))?;\n        if let Event::Key(key) = rx.recv()? {\n            match key.code {\n                KeyCode::Char('q') => return Ok(()),\n                _ => handle_input(key),\n            }\n        }\n    }\n}\n```\n\nThis separates event capture from rendering cleanly."},
    {"role": "user", "content": "Good start. Now I need to add async support — the app needs to fetch data from an API while staying responsive. Also, I found a bug where Ctrl+C doesn't clean up the terminal properly on macOS."},
    {"role": "assistant", "content": "For async, wrap with tokio:\n\n```rust\n#[tokio::main]\nasync fn main() -> Result<()> {\n    let (tx, rx) = mpsc::channel(100);\n    tokio::spawn(async move {\n        let data = reqwest::get(\"https://api.example.com/data\").await?.text().await?;\n        tx.send(Event::DataReady(data)).await?;\n    });\n}\n```\n\nFor the Ctrl+C cleanup bug in `src/event_loop.rs:47`, you need to restore terminal mode:\n\n```rust\nfn restore_terminal() -> Result<()> {\n    disable_raw_mode()?;\n    stdout().execute(LeaveAlternateScreen)?;\n}\n```\n\nCall this in a `ctrlc::set_handler()` and also in Drop for your app struct."},
    {"role": "user", "content": "Perfect. Now I need to implement a security fix — we found that user input in the search bar was being passed directly to a shell command. I need to sanitize the input properly. Also, the file at `/etc/rustycode/config.toml` needs to be migrated to the new format."},
    {"role": "assistant", "content": "For the shell injection fix in `src/tools/bash.rs:128`, use proper escaping:\n\n```rust\nfn sanitize_shell_input(input: &str) -> Result<String> {\n    if input.contains(|c: char| !c.is_alphanumeric() && c != ' ' && c != '-' && c != '_') {\n        return Err(anyhow!(\"Invalid characters in input\"));\n    }\n    Ok(shlex::try_quote(input)?.to_string())\n}\n```\n\nFor the config migration at `/etc/rustycode/config.toml`, use serde's rename attributes for backward compat:\n\n```rust\n#[derive(Deserialize)]\nstruct Config {\n    #[serde(rename = \"api_endpoint\", alias = \"api_url\")]\n    endpoint: String,\n}\n```\n\nNext steps: 1) Add unit tests for sanitize_shell_input with attack payloads, 2) Write config migration script, 3) Update integration tests."},
]

def send_request(model, system_prompt, messages, tools=None, timeout=30):
    url = f"{BASE_URL}/chat/completions"
    body = {
        "model": model,
        "messages": [{"role": "system", "content": system_prompt}] + messages,
        "temperature": 0.3,
        "max_tokens": 1024,
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

def extract_result(resp):
    """Extract text response and tool call from API response."""
    if not resp or "choices" not in resp:
        return None, False, 0, "no response"

    choice = resp["choices"][0]
    msg = choice.get("message", {})
    text = msg.get("content") or ""
    tool_calls = msg.get("tool_calls") or []

    called = False
    summary = ""
    for tc in tool_calls:
        if tc.get("function", {}).get("name") == "compact_context":
            called = True
            try:
                args = json.loads(tc["function"]["arguments"])
                summary = args.get("summary", "")
            except:
                summary = "(parse error)"

    # Quality checks
    quality = 0
    if called and summary:
        # Check key facts preserved
        checks = [
            ("ratatui" in summary.lower() or "tui" in summary.lower(), "framework"),
            ("/etc/rustycode" in summary or "config" in summary.lower(), "file paths"),
            ("sanitize" in summary.lower() or "shell injection" in summary.lower() or "security" in summary.lower(), "security fix"),
            ("test" in summary.lower() or "next step" in summary.lower() or "migration" in summary.lower(), "next steps"),
        ]
        quality = sum(1 for passed, _ in checks if passed)

    return text, called, quality, summary[:80] if summary else ""

# Run tests
results = []
for model in MODELS:
    for suffix_name, suffix_text in [("normal", NORMAL_SUFFIX), ("strong", STRONG_SUFFIX)]:
        label = f"{model}/{suffix_name}"
        print(f"Testing {label}...", end=" ", flush=True)

        resp, err = send_request(model, suffix_text, CONVERSATION, [TOOL_DEF], timeout=60)
        if err:
            print(f"ERROR: {err}")
            results.append({"model": model, "suffix": suffix_name, "status": "error", "error": err})
            continue

        text, called, quality, summary_snip = extract_result(resp)
        text_len = len(text) if text else 0
        status = "CALLED" if called else "SKIPPED"
        print(f"{status} (text={text_len} chars, quality={quality}/4, summary: {summary_snip})")

        results.append({
            "model": model,
            "suffix": suffix_name,
            "status": "called" if called else "skipped",
            "text_chars": text_len,
            "quality": quality,
            "summary_preview": summary_snip,
        })

    time.sleep(2)  # Rate limit buffer

# Edge cases
print("\n--- Edge Cases ---")

EDGE_CASES = {
    "short": [
        {"role": "user", "content": "Hello, how are you?"},
        {"role": "assistant", "content": "I'm doing well! How can I help you today?"},
        {"role": "user", "content": "Can you help me with a quick Rust question?"},
    ],
    "with_tools": [
        {"role": "user", "content": "Check the error logs in /var/log/app.log"},
        {"role": "assistant", "content": None, "tool_calls": [{
            "id": "tc_1", "type": "function",
            "function": {"name": "bash", "arguments": json.dumps({"command": "tail -50 /var/log/app.log"})}
        }]},
        {"role": "tool", "tool_call_id": "tc_1", "content": "2026-05-14 10:23:01 ERROR Connection timeout to db.internal:5432\n2026-05-14 10:23:05 WARN Retrying connection...\n2026-05-14 10:23:10 ERROR Max retries exceeded"},
        {"role": "assistant", "content": "The logs show repeated connection timeouts to `db.internal:5432`. This looks like a database connectivity issue. Let me check the network configuration."},
        {"role": "user", "content": "Yes check the network config and also the firewall rules at /etc/iptables/rules.v4"},
    ],
    "non_english": [
        {"role": "user", "content": "日本語でRustのエラー処理について説明してください"},
        {"role": "assistant", "content": "Rustのエラー処理には主に2つのアプローチがあります：\n\n1. `Result<T, E>` — 回復可能なエラー\n2. `panic!` — 回復不可能なエラー\n\n```rust\nfn read_file(path: &str) -> Result<String, std::io::Error> {\n    std::fs::read_to_string(path)\n}\n```\n\n`?`演算子を使うと簡潔に書けます。"},
        {"role": "user", "content": "ありがとうございます。次にTokioを使った非同期処理について教えてください。特にチャンネルの使い方。"},
    ],
}

for model in MODELS:
    for case_name, conv in EDGE_CASES.items():
        label = f"{model}/{case_name}"
        print(f"Testing {label}...", end=" ", flush=True)
        resp, err = send_request(model, NORMAL_SUFFIX, conv, [TOOL_DEF], timeout=60)
        if err:
            print(f"ERROR: {err}")
            results.append({"model": model, "suffix": "normal", "case": case_name, "status": "error", "error": err})
            continue

        text, called, quality, summary_snip = extract_result(resp)
        text_len = len(text) if text else 0
        status = "CALLED" if called else "SKIPPED"
        print(f"{status} (text={text_len} chars)")

        results.append({
            "model": model, "suffix": "normal", "case": case_name,
            "status": "called" if called else "skipped",
            "text_chars": text_len,
        })
    time.sleep(2)

# Summary
print("\n=== SUMMARY ===")
main_tests = [r for r in results if "case" not in r]
edge_tests = [r for r in results if "case" in r]

for model in MODELS:
    model_main = [r for r in main_tests if r["model"] == model]
    called = sum(1 for r in model_main if r["status"] == "called")
    total = len(model_main)
    avg_q = sum(r.get("quality", 0) for r in model_main if r["status"] == "called") / max(1, called)
    print(f"  {model}: {called}/{total} called, avg quality={avg_q:.1f}/4")

    model_edge = [r for r in edge_tests if r["model"] == model]
    for r in model_edge:
        print(f"    edge/{r['case']}: {r['status']}")

# Save report
os.makedirs(".reports/piggyback", exist_ok=True)
with open(".reports/piggyback/piggyback_report.json", "w") as f:
    json.dump({"timestamp": time.strftime("%Y-%m-%dT%H:%M:%S"), "tests": results}, f, indent=2)
print(f"\nReport saved to .reports/piggyback/piggyback_report.json")
