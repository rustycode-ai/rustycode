#!/usr/bin/env bash
# Piggyback Compaction Multi-Provider Test
#
# Tests piggyback compaction prompts across z.ai and OpenRouter free models.
# Uses realistic multi-turn conversation with thinking blocks, tool calls, errors.
#
# Usage: ./test_piggyback_multi.sh [--quick] [--provider <name>] [--model <model>]
#   --quick       Only test 2 strategies (refined, explicit) instead of all 4
#   --provider    Only test specific provider (zai, openrouter)
#   --model       Only test specific model (e.g., --model glm-5.1)
#   --edge-cases  Also run edge case tests

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPORT_DIR="${SCRIPT_DIR}/../.reports/piggyback"
mkdir -p "$REPORT_DIR"

echo "=== Piggyback Multi-Provider Test ==="
echo "Report dir: $REPORT_DIR"
echo ""

# Delegate everything to Python
python3 - "$@" << 'PYEOF'
import json
import os
import sys
import time
import urllib.request
import urllib.error
import argparse
from datetime import datetime

# Parse args
parser = argparse.ArgumentParser()
parser.add_argument("--quick", action="store_true")
parser.add_argument("--provider", choices=["zai", "openrouter"])
parser.add_argument("--model", type=str)
parser.add_argument("--edge-cases", action="store_true")
args = parser.parse_args()

# Load API config
config_path = os.path.expanduser("~/.rustycode/config.json")
with open(config_path) as f:
    config = json.load(f)

providers = config.get("providers", {})

# Build provider configs
PROVIDERS = {}

# z.ai (openai provider)
zai_cfg = providers.get("openai", {})
if isinstance(zai_cfg, dict) and zai_cfg.get("base_url") and zai_cfg.get("api_key"):
    PROVIDERS["zai"] = {
        "base_url": zai_cfg["base_url"].rstrip("/"),
        "api_key": zai_cfg["api_key"],
        "models": zai_cfg.get("models", ["glm-5.1", "glm-4.7", "glm-4.5-air"]),
    }

# OpenRouter
or_cfg = providers.get("openrouter", {})
if isinstance(or_cfg, dict) and or_cfg.get("api_key"):
    PROVIDERS["openrouter"] = {
        "base_url": "https://openrouter.ai/api/v1",
        "api_key": or_cfg["api_key"],
        "models": [
            "deepseek/deepseek-v4-flash:free",
            "meta-llama/llama-3.3-70b-instruct:free",
            "qwen/qwen3-coder:free",
            "google/gemma-4-31b-it:free",
        ],
    }

# Filter by --provider or --model
if args.provider:
    PROVIDERS = {k: v for k, v in PROVIDERS.items() if k == args.provider}
if args.model:
    for pname, pcfg in PROVIDERS.items():
        pcfg["models"] = [m for m in pcfg["models"] if args.model in m]

print(f"Providers: {list(PROVIDERS.keys())}")
for pname, pcfg in PROVIDERS.items():
    print(f"  {pname}: {pcfg['models']}")
print()

# --- Conversation fixtures ---

# Standard multi-turn conversation
STANDARD_CONVERSATION = [
    {
        "role": "user",
        "content": "I need to refactor the auth module in my Rust project. The current implementation has JWT handling, session management, and OAuth2 all mixed together in one 800-line file. I want to split it into separate modules."
    },
    {
        "role": "assistant",
        "content": "I'll help you refactor the auth module. Let me start by reading the current implementation.\n\n<thinking>\nUser wants to split a large auth module. I should:\n1. Read the current file\n2. Identify natural module boundaries\n3. Plan the split before making changes\n</thinking>"
    },
    {
        "role": "user",
        "content": "[Read tool result: src/auth/mod.rs - 812 lines]\nLines 1-50: imports\nLines 51-180: JWT token generation/validation\nLines 181-320: Session management (Redis-backed)\nLines 321-480: OAuth2 client"
    },
    {
        "role": "assistant",
        "content": "Clear boundaries. My plan:\n1. `jwt.rs` — JWT (lines 51-180)\n2. `session.rs` — Sessions (lines 181-320)\n3. `oauth.rs` — OAuth2 (lines 321-480)\n\n<thinking>\nDependency chain: middleware depends on jwt + session + oauth. Start with leaf modules.\n</thinking>"
    },
    {
        "role": "user",
        "content": "Yes start with jwt.rs. Also we should add refresh token rotation — the current refresh logic has a security issue where old tokens aren't invalidated."
    },
    {
        "role": "assistant",
        "content": "I'll extract jwt.rs and add rotation. Reading the JWT section first.\n\n<thinking>\nNew requirement: refresh token rotation. Need token family tracking, invalidate old tokens on use, detect reuse.\n</thinking>"
    },
    {
        "role": "user",
        "content": "[Read tool result: src/auth/mod.rs lines 51-180]\npub fn generate_token(...) -> Result<String, AuthError> { ... }\npub fn refresh_token(old_token, secret) -> Result<(String, String), AuthError> {\n    let old_claims = validate_token(old_token, secret)?;\n    // BUG: doesn't invalidate old token\n    let new_refresh = generate_token(&old_claims.sub, secret)?; // same expiry!\n    Ok((new_access, new_refresh))\n}"
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
        "content": "jwt.rs done. Writing tests before extracting session.rs.\n\n<thinking>\nTests for rotation logic: reuse detection, family revocation. Then check if OAuth2 depends on JWT.\n</thinking>"
    },
    {
        "role": "user",
        "content": "Good. Write jwt tests and extract session.rs."
    }
]

# Edge case: short conversation
SHORT_CONVERSATION = [
    {
        "role": "user",
        "content": "Fix the typo in the README.md file"
    },
    {
        "role": "assistant",
        "content": "I'll fix the typo. Let me read the file first."
    },
    {
        "role": "user",
        "content": "Go ahead"
    }
]

# Edge case: conversation with existing tool call
TOOL_CALL_CONVERSATION = [
    {
        "role": "user",
        "content": "List files in the src directory"
    },
    {
        "role": "assistant",
        "content": None,
        "tool_calls": [{
            "id": "call_001",
            "type": "function",
            "function": {
                "name": "bash",
                "arguments": json.dumps({"command": "ls src/"})
            }
        }]
    },
    {
        "role": "tool",
        "tool_call_id": "call_001",
        "content": "main.rs\nlib.rs\nutils.rs"
    },
    {
        "role": "user",
        "content": "Now read lib.rs and explain what it does"
    },
    {
        "role": "assistant",
        "content": "Reading lib.rs now.\n\n<thinking>\nUser wants explanation of lib.rs. I should read it and provide a clear summary of the public API and module structure.\n</thinking>"
    },
    {
        "role": "user",
        "content": "Also check if there are any TODO comments in the codebase"
    },
    {
        "role": "assistant",
        "content": "I'll search for TODO comments while explaining lib.rs."
    },
    {
        "role": "user",
        "content": "Good, what about fixing the ones in lib.rs first?"
    }
]

# Edge case: non-English
NON_ENGLISH_CONVERSATION = [
    {
        "role": "user",
        "content": "我需要修复 Rust 项目中的认证模块。当前的 JWT 刷新逻辑有安全漏洞。"
    },
    {
        "role": "assistant",
        "content": "我来帮你修复JWT刷新逻辑的安全问题。\n\n<thinking>\n用户提到JWT刷新逻辑有安全漏洞。需要先读取代码，理解当前的实现方式。\n</thinking>"
    },
    {
        "role": "user",
        "content": "主要问题是旧的 refresh token 没有被撤销，而且 refresh token 的过期时间和 access token 一样。"
    },
    {
        "role": "assistant",
        "content": "理解了，需要实现 token rotation 和 family tracking。让我先看一下代码。"
    },
    {
        "role": "user",
        "content": "好的，src/auth/jwt.rs 是主要需要修改的文件。"
    },
    {
        "role": "assistant",
        "content": "正在读取 jwt.rs...\n\n<thinking>\n需要添加：1. TokenFamily 追踪 2. 旧 token 撤销 3. 复用检测\n</thinking>"
    },
    {
        "role": "user",
        "content": "写好之后记得加上测试。"
    }
]

# --- Tool definition ---
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
                    "description": "What the user is trying to accomplish overall — their high-level intent."
                },
                "progress": {
                    "type": "string",
                    "description": "What has been completed AND what is still in-progress. Include files read/edited, tool calls and results, errors found and fixed."
                },
                "decisions": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Important decisions still in effect and why."
                },
                "active_files": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "File paths with unresolved changes or central to current work."
                },
                "next_step": {
                    "type": "string",
                    "description": "The single most important next step."
                }
            },
            "required": ["goal", "progress", "next_step"]
        }
    }
}

# --- Prompt strategies ---
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

if args.quick:
    STRATEGIES = {k: v for k, v in STRATEGIES.items() if k in ("refined", "explicit")}

# --- API helpers ---

def send_request(base_url, api_key, model, system_prompt, messages, tools=None):
    """Send request and return (response_data, error_string)."""
    url = f"{base_url}/chat/completions"

    request_messages = [{"role": "system", "content": system_prompt}] + messages

    request_body = {
        "model": model,
        "messages": request_messages,
        "tools": [tools] if tools else [],
        "tool_choice": "auto",
        "max_tokens": 4096,
    }

    # Remove empty tools array (some models don't support it)
    if not request_body["tools"]:
        del request_body["tools"]
        del request_body["tool_choice"]

    data = json.dumps(request_body).encode()
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}",
    }

    # OpenRouter needs additional headers
    if "openrouter" in base_url:
        headers["HTTP-Referer"] = "https://rustycode.ai"
        headers["X-Title"] = "RustyCode Piggyback Test"

    req = urllib.request.Request(url, data=data, headers=headers)

    try:
        with urllib.request.urlopen(req, timeout=90) as resp:
            return json.loads(resp.read()), None
    except urllib.error.HTTPError as e:
        body = e.read().decode()[:500]
        return None, f"HTTP {e.code}: {body}"
    except Exception as e:
        return None, str(e)


def analyze_response(data, model, strategy):
    """Analyze response and return result dict."""
    result = {
        "model": model,
        "strategy": strategy,
        "tool_called": False,
        "response_chars": 0,
        "quality": {},
        "tokens": {},
        "error": None,
        "summary_preview": None,
    }

    if not data:
        result["error"] = "No response data"
        return result

    choices = data.get("choices", [])
    if not choices:
        result["error"] = "No choices returned"
        return result

    message = choices[0].get("message", {})
    content = message.get("content", "") or ""
    tool_calls = message.get("tool_calls", [])

    result["response_chars"] = len(content)

    compact_calls = [
        tc for tc in tool_calls
        if tc.get("function", {}).get("name") == "compact_context"
    ]

    result["tool_called"] = len(compact_calls) > 0

    if compact_calls:
        try:
            args = json.loads(compact_calls[0]["function"]["arguments"])
            result["summary_preview"] = {
                "goal": args.get("goal", "")[:60],
                "next_step": args.get("next_step", "")[:60],
                "decisions_count": len(args.get("decisions", [])),
                "files": args.get("active_files", []),
            }

            rendered = json.dumps(args).lower()
            result["quality"] = {
                "file_paths": any(p in rendered for p in ["jwt.rs", "mod.rs", "session.rs", "auth"]),
                "security_fix": any(k in rendered for k in ["refresh", "rotation", "invalidate", "reuse", "token"]),
                "refactor_plan": any(k in rendered for k in ["split", "extract", "refactor", "module"]),
                "goal_captured": len(args.get("goal", "")) > 20,
                "next_step_specific": len(args.get("next_step", "")) > 15,
            }
        except (json.JSONDecodeError, KeyError) as e:
            result["error"] = f"Tool call parse error: {e}"

    usage = data.get("usage", {})
    result["tokens"] = {
        "prompt": usage.get("prompt_tokens", "?"),
        "completion": usage.get("completion_tokens", "?"),
    }

    return result


# --- Main test runner ---

all_results = []

def test_model_strategy(base_url, api_key, model, strategy_name, system_prompt,
                        conversation=None, conversation_name="standard"):
    """Run a single test and return result."""
    conv = conversation or STANDARD_CONVERSATION

    data, error = send_request(base_url, api_key, model, system_prompt, conv, TOOL_DEF)

    if error:
        print(f"    ERROR: {error[:100]}")
        return {
            "model": model,
            "strategy": strategy_name,
            "conversation": conversation_name,
            "tool_called": False,
            "error": error[:200],
        }

    result = analyze_response(data, model, strategy_name)
    result["conversation"] = conversation_name

    status = "CALLED" if result["tool_called"] else "SKIP"
    resp_chars = result["response_chars"]

    quality_str = ""
    if result["tool_called"] and result.get("quality"):
        q = result["quality"]
        passed = sum(1 for v in q.values() if v)
        total = len(q)
        quality_str = f" quality={passed}/{total}"

    print(f"    {strategy_name}: {status} ({resp_chars} chars){quality_str}")
    return result


def run_conversation_tests(base_url, api_key, model, provider_name):
    """Run all strategies for a model."""
    print(f"\n  --- {provider_name}/{model} ---")

    for sname, sprompt in STRATEGIES.items():
        result = test_model_strategy(base_url, api_key, model, sname, sprompt)
        all_results.append(result)
        time.sleep(2)  # Rate limit cushion

    if args.edge_cases:
        print(f"    [edge cases]")
        for sname in ["explicit", "mandatory"]:
            sprompt = STRATEGIES[sname]
            # Short conv
            r = test_model_strategy(base_url, api_key, model, f"{sname}_short", sprompt,
                                   SHORT_CONVERSATION, "short")
            all_results.append(r)
            time.sleep(2)

            # Tool call conv
            r = test_model_strategy(base_url, api_key, model, f"{sname}_tools", sprompt,
                                   TOOL_CALL_CONVERSATION, "with_tools")
            all_results.append(r)
            time.sleep(2)

            # Non-English
            r = test_model_strategy(base_url, api_key, model, f"{sname}_i18n", sprompt,
                                   NON_ENGLISH_CONVERSATION, "non_english")
            all_results.append(r)
            time.sleep(2)


# Run all providers and models
start_time = time.time()

for provider_name, pcfg in PROVIDERS.items():
    print(f"\n=== Provider: {provider_name} ===")
    for model in pcfg["models"]:
        try:
            run_conversation_tests(pcfg["base_url"], pcfg["api_key"], model, provider_name)
        except KeyboardInterrupt:
            print("\nInterrupted!")
            sys.exit(1)
        except Exception as e:
            print(f"  FATAL: {e}")
            continue

elapsed = time.time() - start_time

# --- Generate report ---

print(f"\n{'='*60}")
print(f"RESULTS SUMMARY ({elapsed:.0f}s)")
print(f"{'='*60}")

# Group by model
by_model = {}
for r in all_results:
    m = r["model"]
    if m not in by_model:
        by_model[m] = []
    by_model[m].append(r)

print(f"\n{'Model':<40} {'Called':>7} {'Total':>7} {'Rate':>7}")
print("-" * 65)

for model, results in sorted(by_model.items()):
    standard = [r for r in results if r.get("conversation") == "standard"]
    if not standard:
        continue
    called = sum(1 for r in standard if r.get("tool_called"))
    total = len(standard)
    rate = called / total * 100 if total > 0 else 0
    print(f"{model:<40} {called:>7} {total:>7} {rate:>6.0f}%")

# Best strategy analysis
print(f"\n{'Strategy':<15} {'Called':>7} {'Total':>7} {'Rate':>7}")
print("-" * 40)
for sname in STRATEGIES:
    matching = [r for r in all_results if r["strategy"] == sname and r.get("conversation") == "standard"]
    called = sum(1 for r in matching if r.get("tool_called"))
    total = len(matching)
    rate = called / total * 100 if total > 0 else 0
    print(f"{sname:<15} {called:>7} {total:>7} {rate:>6.0f}%")

# Quality summary (for successful calls)
quality_results = [r for r in all_results if r.get("tool_called") and r.get("quality")]
if quality_results:
    print(f"\nQuality (across {len(quality_results)} successful calls):")
    for check in ["file_paths", "security_fix", "refactor_plan", "goal_captured", "next_step_specific"]:
        passed = sum(1 for r in quality_results if r["quality"].get(check))
        total = len(quality_results)
        print(f"  {check:<20} {passed:>3}/{total:<3} ({passed/total*100:.0f}%)")

# Edge case summary
edge_results = [r for r in all_results if r.get("conversation") != "standard"]
if edge_results:
    print(f"\nEdge cases ({len(edge_results)} tests):")
    edge_by_conv = {}
    for r in edge_results:
        cn = r.get("conversation", "unknown")
        if cn not in edge_by_conv:
            edge_by_conv[cn] = []
        edge_by_conv[cn].append(r)
    for conv_name, results in edge_by_conv.items():
        called = sum(1 for r in results if r.get("tool_called"))
        total = len(results)
        print(f"  {conv_name:<20} {called}/{total} called")

# Save full report
report_path = os.path.join(os.environ.get("REPORT_DIR", "."), "piggyback_report.json")
with open(report_path, "w") as f:
    json.dump({
        "timestamp": datetime.utcnow().isoformat(),
        "elapsed_seconds": elapsed,
        "total_tests": len(all_results),
        "results": all_results,
    }, f, indent=2)
print(f"\nFull report: {report_path}")
PYEOF
