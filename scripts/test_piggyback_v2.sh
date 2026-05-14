#!/usr/bin/env bash
# Piggyback Compaction Prompt Experiment v2
#
# Tests multiple prompt strategies for getting the LLM to call compact_context
# alongside its normal response. Uses a realistic multi-turn conversation with
# thinking blocks, tool calls, and interleaved results.
#
# Usage: ./test_piggyback_v2.sh [model]

set -euo pipefail

MODEL="${1:-glm-4.5-air}"

echo "=== Piggyback Prompt Experiment v2 ==="
echo "Model: $MODEL"
echo ""

# Delegate everything to Python to avoid bash quoting nightmares
python3 - "$MODEL" << 'PYEOF'
import json
import os
import sys
import time
import urllib.request
import urllib.error

model = sys.argv[1] if len(sys.argv) > 1 else "glm-4.5-air"

# Load API config
config_path = os.path.expanduser("~/.rustycode/config.json")
with open(config_path) as f:
    config = json.load(f)

providers = config.get("providers", {})
api_base = None
api_key = None
for name, cfg in providers.items():
    if cfg is None or not isinstance(cfg, dict):
        continue
    base = cfg.get("base_url", "")
    key = cfg.get("api_key", "")
    if base and key:
        api_base = base.rstrip("/")
        api_key = key
        print(f"Using provider: {name}")
        break

if not api_base or not api_key:
    print("ERROR: No provider with base_url + api_key found in config")
    sys.exit(1)

url = f"{api_base}/chat/completions"

# Realistic multi-turn conversation with thinking, tool calls, errors
CONVERSATION = [
    {
        "role": "user",
        "content": "I need to refactor the auth module in my Rust project. The current implementation has JWT handling, session management, and OAuth2 all mixed together in one 800-line file. I want to split it into separate modules."
    },
    {
        "role": "assistant",
        "content": "I'll help you refactor the auth module. Let me start by reading the current implementation.\n\n<thinking>\nUser wants to split a large auth module. I should:\n1. Read the current file\n2. Identify natural module boundaries\n3. Plan the split before making changes\n4. Ensure tests pass after each move\n</thinking>"
    },
    {
        "role": "user",
        "content": "[Read tool result: src/auth/mod.rs - 812 lines]\nLines 1-50: imports\nLines 51-180: JWT token generation/validation\nLines 181-320: Session management (Redis-backed)\nLines 321-480: OAuth2 client\nLines 481-600: Auth middleware\nLines 601-720: User repository\nLines 721-812: Tests"
    },
    {
        "role": "assistant",
        "content": "Clear boundaries. My plan:\n1. `jwt.rs` — JWT (lines 51-180)\n2. `session.rs` — Sessions (lines 181-320)\n3. `oauth.rs` — OAuth2 (lines 321-480)\n4. `middleware.rs` — Auth middleware (lines 481-600)\n5. `repository.rs` — User repo (lines 601-720)\n6. `mod.rs` — Re-exports\n\n<thinking>\nDependency chain: middleware depends on jwt + session + oauth. All depend on repository. Start with leaf modules.\n</thinking>"
    },
    {
        "role": "user",
        "content": "Yes start with jwt.rs. Also we should add refresh token rotation — the current refresh logic has a security issue where old tokens aren't invalidated."
    },
    {
        "role": "assistant",
        "content": "I'll extract jwt.rs and add rotation. Reading the JWT section first.\n\n<thinking>\nNew requirement mid-refactor: refresh token rotation. Need:\n1. Token family tracking\n2. Invalidate old refresh tokens on use\n3. Detect reuse (possible theft)\n</thinking>"
    },
    {
        "role": "user",
        "content": "[Read tool result: src/auth/mod.rs lines 51-180]\npub fn generate_token(...) -> Result<String, AuthError> { ... }\npub fn validate_token(...) -> Result<Claims, AuthError> { ... }\npub fn refresh_token(old_token, secret) -> Result<(String, String), AuthError> {\n    let old_claims = validate_token(old_token, secret)?;\n    // BUG: doesn't invalidate old token\n    let new_access = generate_token(&old_claims.sub, secret)?;\n    let new_refresh = generate_token(&old_claims.sub, secret)?; // same expiry!\n    Ok((new_access, new_refresh))\n}"
    },
    {
        "role": "assistant",
        "content": "Bug found:\n1. Old token not invalidated\n2. Refresh has same 1hr expiry as access (should be 7d)\n3. No family tracking, no reuse detection\n\nCreating jwt.rs with TokenFamily, rotation, and reuse detection."
    },
    {
        "role": "user",
        "content": "[Write tool result: src/auth/jwt.rs created - 210 lines]\nTokenFamily tracking, refresh rotation, reuse detection, proper expiry separation (access: 1hr, refresh: 7d)"
    },
    {
        "role": "assistant",
        "content": "jwt.rs done. Now writing tests before extracting session.rs.\n\n<thinking>\nTests first for rotation logic — edge cases: reuse detection, family revocation. Then check if OAuth2 depends on JWT functions.\n</thinking>"
    },
    {
        "role": "user",
        "content": "Write the tests, then check if OAuth2 uses JWT functions and extract session.rs."
    },
    {
        "role": "assistant",
        "content": "[Grep result: src/auth/mod.rs lines 321-480]\n3 JWT references in OAuth2: generate_token (line 345), validate_token (line 402, 478). Only public API — import path: `use super::jwt::{generate_token, validate_token}`.\n\nWriting jwt tests now."
    },
    {
        "role": "user",
        "content": "Good. Write jwt tests and extract session.rs."
    }
]

# Tool definition — refined prompt
TOOL_DEF = {
    "type": "function",
    "function": {
        "name": "compact_context",
        "description": (
            "The conversation context is approaching capacity. Call this tool ALONGSIDE "
            "your normal response — your answer to the user is the priority, this is a "
            "side-effect. The summary will replace older messages on the next turn.\n\n"
            "Capture: the user's goal, what has been done, what is still in-progress, "
            "active files, key decisions, and the immediate next step. Include reasoning "
            "ONLY when it explains a non-obvious decision or an incomplete approach that "
            "the next turn needs to continue. Exclude: system instructions, tool "
            "descriptions, obsolete reasoning, completed tasks that are no longer relevant."
        ),
        "parameters": {
            "type": "object",
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "What the user is trying to accomplish overall — their high-level intent, not just the last message."
                },
                "progress": {
                    "type": "string",
                    "description": "What has been completed AND what is still in-progress. Include files read/edited, tool calls and results, errors found and fixed. Only include reasoning when it explains a non-obvious decision the next turn will need."
                },
                "decisions": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Important decisions still in effect and why. Exclude decisions that have been superseded or are no longer relevant."
                },
                "active_files": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "File paths with unresolved changes or that are central to the current in-progress work."
                },
                "next_step": {
                    "type": "string",
                    "description": "The single most important next step to continue the current in-progress work."
                }
            },
            "required": ["goal", "progress", "next_step"]
        }
    }
}

# Prompt strategies to test
STRATEGIES = {
    "soft": (
        "You are a helpful coding assistant with access to tools."
    ),
    "refined": (
        "You are a helpful coding assistant with access to tools. "
        "The compact_context tool is available for managing conversation length."
    ),
    "explicit": (
        "You are a helpful coding assistant with access to tools. "
        "[Context Management] The conversation has grown long. A compact_context tool "
        "has been added to your available tools. Please call it alongside your response "
        "to this message so that older turns can be condensed. Your response to the "
        "user is the priority — the tool call is a secondary side-effect."
    ),
    "mandatory": (
        "You are a helpful coding assistant with access to tools. "
        "IMPORTANT: You MUST call the compact_context tool in your response alongside "
        "answering the user. This is required to free context space. Answer the user "
        "FIRST, then call compact_context."
    ),
}

def send_request(system_prompt, strategy_name):
    """Send request and return parsed response."""
    messages = [{"role": "system", "content": system_prompt}] + CONVERSATION

    request_body = {
        "model": model,
        "messages": messages,
        "tools": [TOOL_DEF],
        "tool_choice": "auto",
        "max_tokens": 4096,
    }

    data = json.dumps(request_body).encode()
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        },
    )

    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return json.loads(resp.read())
    except urllib.error.HTTPError as e:
        body = e.read().decode()[:200]
        print(f"  HTTP {e.code}: {body}")
        return None
    except Exception as e:
        print(f"  Error: {e}")
        return None

def analyze_response(data, strategy_name):
    """Analyze response and print results."""
    if not data:
        return

    choices = data.get("choices", [])
    if not choices:
        print("  No choices returned")
        return

    message = choices[0].get("message", {})
    content = message.get("content", "") or ""
    tool_calls = message.get("tool_calls", [])

    compact_calls = [
        tc for tc in tool_calls
        if tc.get("function", {}).get("name") == "compact_context"
    ]

    print(f"  Response: {len(content)} chars")
    print(f"  Tool calls: {len(tool_calls)}")
    print(f"  compact_context: {'YES' if compact_calls else 'NO'}")

    if compact_calls:
        args = json.loads(compact_calls[0]["function"]["arguments"])
        print(f"  Summary:")
        print(f"    goal: {args.get('goal', '')[:80]}...")
        print(f"    progress: {args.get('progress', '')[:80]}...")
        print(f"    decisions: {len(args.get('decisions', []))} items")
        print(f"    active_files: {args.get('active_files', [])}")
        print(f"    next_step: {args.get('next_step', '')[:80]}...")

        # Quality checks
        rendered = json.dumps(args).lower()
        checks = {
            "File paths": any(p in rendered for p in ["jwt.rs", "mod.rs", "session.rs"]),
            "Security fix": any(k in rendered for k in ["refresh", "rotation", "invalidate", "reuse"]),
            "Refactoring plan": any(k in rendered for k in ["split", "extract", "refactor", "module"]),
            "Thinking captured": any(k in rendered for k in ["thinking", "reason", "analysis", "approach", "plan", "strategy", "considered"]),
            "Goal captured": len(args.get("goal", "")) > 20,
            "Next step specific": len(args.get("next_step", "")) > 15,
        }

        all_pass = True
        for check, passed in checks.items():
            status = "PASS" if passed else "FAIL"
            print(f"    [{status}] {check}")
            if not passed:
                all_pass = False

        if all_pass:
            print("  >>> ALL QUALITY CHECKS PASSED <<<")
    else:
        preview = content[:150].replace("\n", " ")
        print(f"  Preview: {preview}...")

    usage = data.get("usage", {})
    if usage:
        print(f"  Tokens: prompt={usage.get('prompt_tokens', '?')}, "
              f"completion={usage.get('completion_tokens', '?')}")

# Run all strategies
print(f"\n=== Testing 4 prompt strategies ===\n")

for name, prompt in STRATEGIES.items():
    print(f"--- Strategy: {name} ---")
    data = send_request(prompt, name)
    analyze_response(data, name)
    print("")
    time.sleep(3)

print("=== Summary ===")
print("  soft     — Neutral prompt (baseline)")
print("  refined  — Mentions compact_context exists")
print("  explicit — System prompt suffix with clear instruction")
print("  mandatory — Direct MANDATORY instruction")
PYEOF
