#!/usr/bin/env python3
"""
Simulates the conversation-hijack relevance scoring concept.

Tests whether injecting an evaluation prompt into a conversation,
getting LLM relevance scores, then stripping the exchange produces
useful metadata for compression decisions.

Usage:
    python3 scripts/test_relevance_eval.py           # mock mode (no API)
    OPENAI_API_KEY=sk-... python3 scripts/test_relevance_eval.py  # live mode
"""

import json
import os
import sys

# ─── Sample conversation (simulating a coding session) ─────────────────────

SAMPLE_CONVERSATION = [
    {"role": "system", "content": "You are a coding assistant working on a Rust project."},
    {"role": "user", "content": "Fix the authentication bug in auth.rs — users can't log in with special characters in passwords"},
    {"role": "assistant", "content": "I found the issue. The password validation regex in auth.rs:42 doesn't escape special characters. The regex `pattern = /^[a-zA-Z0-9]+$/` rejects symbols like !@#$%. I'll fix it to accept all printable characters."},
    {"role": "user", "content": "Good find. Also check if the same bug exists in the registration flow."},
    {"role": "assistant", "content": "Checked registration.rs — same bug at line 78. Both use the same validation function `validate_password()`. I'll fix both by updating the shared validator in password_utils.rs."},
    {"role": "user", "content": "Now run the tests to make sure nothing broke."},
    {"role": "assistant", "content": "Ran `cargo test -p rustycode-auth`. All 12 tests pass. Also added 3 new test cases for special characters: test_password_with_symbols, test_password_with_unicode, test_password_with_spaces."},
    {"role": "user", "content": "Great. Now update the API documentation to reflect the password policy change."},
    {"role": "assistant", "content": "Updated docs/api/auth.md section on password requirements. Changed from 'alphanumeric only' to 'any printable character, min 8 chars'. Also updated the OpenAPI schema example."},
    {"role": "user", "content": "One more thing — add rate limiting to the login endpoint. We're getting brute-force attempts."},
]


def build_eval_prompt(messages: list[dict], current_task: str) -> list[dict]:
    """Build the evaluation prompt to inject into the conversation.

    The prompt asks the LLM to rate each turn's relevance to the current task.
    """
    # Build turn summaries (not full content — keep it lightweight)
    turn_summaries = []
    for i, msg in enumerate(messages):
        content_preview = msg["content"][:120].replace("\n", " ")
        turn_summaries.append(f"Turn {i}: [{msg['role']}] {content_preview}...")

    eval_messages = messages + [
        {
            "role": "user",
            "content": f"""[context-eval] Before continuing with the task, evaluate the conversation so far.

Current task: {current_task}

Conversation turns:
{chr(10).join(turn_summaries)}

For each turn, rate its relevance (1-5) to the current and likely next steps.
Consider: does this turn contain information we'll need later? Is it superseded? Is it still actionable?

Return ONLY a JSON array:
[
  {{"index": 0, "score": <1-5>, "reason": "<brief explanation>"}},
  ...
]

Do not continue the task. Just return the JSON evaluation."""
        }
    ]
    return eval_messages


def parse_scores(response_text: str) -> list[dict]:
    """Parse relevance scores from LLM response, handling various formats."""
    # Try to extract JSON from the response
    text = response_text.strip()

    # Remove markdown code fences if present
    if "```json" in text:
        text = text.split("```json")[1].split("```")[0].strip()
    elif "```" in text:
        text = text.split("```")[1].split("```")[0].strip()

    # Try direct JSON parse
    try:
        scores = json.loads(text)
        if isinstance(scores, list):
            return scores
    except json.JSONDecodeError:
        pass

    # Try to find JSON array in text
    start = text.find("[")
    end = text.rfind("]") + 1
    if start >= 0 and end > start:
        try:
            scores = json.loads(text[start:end])
            if isinstance(scores, list):
                return scores
        except json.JSONDecodeError:
            pass

    return []


def inject_and_evaluate(messages: list[dict], current_task: str) -> tuple[list[dict], list[dict]]:
    """Simulate the inject→evaluate→strip→score flow.

    Returns:
        (clean_messages_with_scores, eval_exchange)
    """
    print("=" * 70)
    print("PHASE 1: Inject evaluation prompt")
    print("=" * 70)

    eval_messages = build_eval_prompt(messages, current_task)
    print(f"  Conversation has {len(messages)} turns")
    print(f"  Injected eval prompt as turn {len(messages)}")
    print(f"  Total messages sent to LLM: {len(eval_messages)}")
    print()

    return eval_messages, []


def simulate_llm_response(messages: list[dict]) -> str:
    """Simulate what an LLM would return for the evaluation prompt.

    In mock mode, we generate realistic scores based on conversation content.
    """
    # Analyze conversation to produce realistic scores
    scores = []
    for i, msg in enumerate(messages[:-1]):  # Skip the injected eval prompt
        content = msg["content"].lower()
        role = msg["role"]

        # Heuristic scoring (simulating LLM judgment)
        score = 3  # default
        reason = "general context"

        if role == "system":
            score = 5
            reason = "system prompt — always relevant"
        elif "bug" in content or "fix" in content or "auth" in content:
            if i <= 3:  # Early bug-related turns
                score = 5
                reason = "core bug investigation — root cause still relevant"
            else:
                score = 2
                reason = "bug already fixed — superseded"
        elif "test" in content and ("pass" in content or "cargo test" in content):
            score = 4
            reason = "test results — confirms fix works"
        elif "doc" in content or "documentation" in content or "api" in content:
            score = 3
            reason = "documentation update — completed, lower priority"
        elif "rate limit" in content or "brute" in content:
            score = 5
            reason = "current active task — not yet addressed"
        elif role == "user" and i <= 1:
            score = 5
            reason = "original task definition"

        scores.append({"index": i, "score": score, "reason": reason})

    return json.dumps(scores, indent=2)


def load_config() -> dict:
    """Load credentials from ~/.rustycode/config.json."""
    config_path = os.path.expanduser("~/.rustycode/config.json")
    with open(config_path) as f:
        return json.load(f)


def call_live_llm(messages: list[dict], provider: str = "openai") -> str:
    """Call LLM API with the evaluation messages.

    Reads credentials from ~/.rustycode/config.json.
    Supports: openai (z.ai), anthropic (z.ai), openrouter
    """
    import urllib.request

    config = load_config()

    # Resolve provider config
    provider_config = config.get("providers", {}).get(provider, {})
    api_key = provider_config.get("api_key") or os.environ.get("OPENAI_API_KEY")
    base_url = provider_config.get("base_url", "https://api.openai.com/v1")
    model = config.get("model", "gpt-4o-mini")

    if not api_key:
        print(f"ERROR: No API key for provider '{provider}'")
        sys.exit(1)

    # OpenAI-compatible endpoint
    url = f"{base_url}/chat/completions"
    payload = {
        "model": model,
        "messages": [{"role": m["role"], "content": m["content"]} for m in messages],
        "max_tokens": 1000,
        "temperature": 0.1,
    }

    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
    )

    print(f"  Calling {base_url} ({model})...")
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            result = json.loads(resp.read().decode())
        return result["choices"][0]["message"]["content"]
    except urllib.error.HTTPError as e:
        body = e.read().decode()
        print(f"  API error {e.code}: {body[:200]}")
        sys.exit(1)


def partition_by_scores(messages: list[dict], scores: list[dict]) -> dict:
    """Partition messages into MUST-KEEP / COMPRESSIBLE / DISCARDABLE."""
    score_map = {s["index"]: s["score"] for s in scores}

    must_keep = []
    compressible = []
    discardable = []

    for i, msg in enumerate(messages):
        score = score_map.get(i, 3)  # default to 3 if not scored

        entry = {
            "index": i,
            "role": msg["role"],
            "content_preview": msg["content"][:80] + "..." if len(msg["content"]) > 80 else msg["content"],
            "score": score,
        }

        if score >= 4:
            must_keep.append(entry)
        elif score >= 2:
            compressible.append(entry)
        else:
            discardable.append(entry)

    return {
        "must_keep": must_keep,
        "compressible": compressible,
        "discardable": discardable,
    }


def main():
    live_mode = bool(os.environ.get("OPENAI_API_KEY"))
    mode = "LIVE (OpenAI)" if live_mode else "MOCK (simulated)"

    print()
    print(f"  Conversation Relevance Evaluation — {mode} mode")
    print()

    current_task = "Fix auth bug and add rate limiting to login endpoint"
    messages = SAMPLE_CONVERSATION

    # Phase 1: Build eval prompt
    eval_messages, _ = inject_and_evaluate(messages, current_task)

    # Phase 2: Get LLM response
    print("=" * 70)
    print("PHASE 2: Get LLM evaluation")
    print("=" * 70)

    if live_mode:
        response_text = call_live_llm(eval_messages)
    else:
        response_text = simulate_llm_response(eval_messages)

    print(f"  Raw response ({len(response_text)} chars):")
    print()
    for line in response_text.split("\n")[:30]:
        print(f"    {line}")
    print()

    # Phase 3: Parse scores
    print("=" * 70)
    print("PHASE 3: Parse scores and strip eval exchange")
    print("=" * 70)

    scores = parse_scores(response_text)

    if not scores:
        print("  ERROR: Could not parse scores from response")
        sys.exit(1)

    print(f"  Parsed {len(scores)} turn scores")
    print()
    for s in scores:
        print(f"    Turn {s['index']}: score={s['score']} — {s['reason']}")
    print()

    # Show what gets stripped
    print(f"  Stripping eval exchange (2 messages) from conversation")
    print(f"  Clean conversation: {len(messages)} turns (unchanged)")
    print(f"  Scores stored as metadata on each message")
    print()

    # Phase 4: Partition based on scores
    print("=" * 70)
    print("PHASE 4: Partition messages by relevance")
    print("=" * 70)
    print()

    partition = partition_by_scores(messages, scores)

    print(f"  MUST-KEEP (score >= 4): {len(partition['must_keep'])} turns")
    for entry in partition["must_keep"]:
        print(f"    Turn {entry['index']}: [{entry['role']}] {entry['content_preview']}")
    print()

    print(f"  COMPRESSIBLE (score 2-3): {len(partition['compressible'])} turns")
    for entry in partition["compressible"]:
        print(f"    Turn {entry['index']}: [{entry['role']}] {entry['content_preview']}")
    print()

    print(f"  DISCARDABLE (score < 2): {len(partition['discardable'])} turns")
    for entry in partition["discardable"]:
        print(f"    Turn {entry['index']}: [{entry['role']}] {entry['content_preview']}")
    print()

    # Summary
    total_content = sum(len(m["content"]) for m in messages)
    kept_content = sum(len(messages[e["index"]]["content"]) for e in partition["must_keep"])
    compress_content = sum(len(messages[e["index"]]["content"]) for e in partition["compressible"])
    discard_content = sum(len(messages[e["index"]]["content"]) for e in partition["discardable"])

    print("=" * 70)
    print("SUMMARY")
    print("=" * 70)
    print(f"  Total conversation: {total_content} chars across {len(messages)} turns")
    print(f"  MUST-KEEP:         {kept_content:6d} chars ({kept_content*100//total_content}%)")
    print(f"  COMPRESSIBLE:      {compress_content:6d} chars ({compress_content*100//total_content}%)")
    print(f"  DISCARDABLE:       {discard_content:6d} chars ({discard_content*100//total_content if total_content else 0}%)")
    print()

    if live_mode:
        print("  Live API test completed successfully!")
        print("  The LLM returned parseable relevance scores.")
    else:
        print("  Mock mode. Run with OPENAI_API_KEY=sk-... for live test.")
    print()


if __name__ == "__main__":
    main()
