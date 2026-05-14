#!/usr/bin/env bash
# Experiment: test piggyback compact_context tool with Gemini API.
#
# Sends a realistic coding conversation + the compact_context tool definition.
# Measures whether the LLM:
#   1. Calls the compact_context tool alongside its normal response
#   2. Produces a useful structured summary
#   3. Preserves important info (file paths, decisions, next steps)

set -euo pipefail

# Load API key from config (don't echo it)
API_KEY=$(python3 -c "
import json
with open('$HOME/.rustycode/config.json') as f:
    d = json.load(f)
gemini = d.get('providers', {}).get('gemini', {})
print(gemini.get('api_key', ''))
")

if [ -z "$API_KEY" ]; then
    echo "ERROR: No Gemini API key found in config"
    exit 1
fi

MODEL="${1:-gemini-2.5-flash}"
URL="https://generativelanguage.googleapis.com/v1beta/models/${MODEL}:generateContent?key=${API_KEY}"

echo "=== Piggyback Compaction Experiment ==="
echo "Model: $MODEL"
echo ""

# Build the request JSON
# Simulates a long coding conversation about fixing an auth bug
REQUEST=$(cat <<'PAYLOAD'
{
  "system_instruction": {
    "parts": [
      {"text": "You are a helpful coding assistant. You have access to tools. When the compact_context tool is available, you may call it to summarize the conversation alongside your normal response."}
    ]
  },
  "contents": [
    {
      "role": "user",
      "parts": [{"text": "I need to fix the authentication bug in my Rust project. The JWT tokens are not being refreshed properly."}]
    },
    {
      "role": "model",
      "parts": [{"text": "I'll help you fix the JWT refresh bug. Let me start by reading the auth module to understand the current implementation."}]
    },
    {
      "role": "user",
      "parts": [{"text": "[Read tool result: src/auth/jwt.rs - 150 lines]\nuse jsonwebtoken::{encode, decode, Header, Validation};\nuse serde::{Deserialize, Serialize};\nuse chrono::{Utc, Duration};\n\n#[derive(Serialize, Deserialize)]\npub struct Claims {\n    sub: String,\n    exp: usize,\n    iat: usize,\n}\n\npub fn generate_token(user_id: &str, secret: &str) -> Result<String, AuthError> {\n    let expiration = Utc::now()\n        .checked_add_signed(Duration::hours(24))\n        .expect(\"valid timestamp\")\n        .timestamp() as usize;\n    let claims = Claims {\n        sub: user_id.to_owned(),\n        exp: expiration,\n        iat: Utc::now().timestamp() as usize,\n    };\n    encode(&Header::default(), &claims, secret.as_ref())\n        .map_err(|e| AuthError::TokenGeneration(e.to_string()))\n}\n\npub fn validate_token(token: &str, secret: &str) -> Result<Claims, AuthError> {\n    let data = decode::<Claims>(token, secret.as_ref(), &Validation::default())?;\n    Ok(data.claims)\n}\n... 130 more lines of code including refresh_token, revoke_token, etc."}]
    },
    {
      "role": "model",
      "parts": [{"text": "I found the issue. In `refresh_token()`, the new token's `iat` (issued-at) is not being updated — it keeps the old token's `iat` value. This means the refreshed token appears to be stale. Let me read the middleware to check how it validates tokens."}]
    },
    {
      "role": "user",
      "parts": [{"text": "[Read tool result: src/middleware/auth.rs - 80 lines]\npub async fn auth_middleware(\n    req: Request,\n    next: Next,\n) -> Result<Response, AuthError> {\n    let auth_header = req\n        .headers()\n        .get(\"Authorization\")\n        .and_then(|v| v.to_str().ok())\n        .ok_or(AuthError::MissingToken)?;\n    \n    let token = auth_header\n        .strip_prefix(\"Bearer \")\n        .ok_or(AuthError::InvalidFormat)?;\n    \n    let claims = jwt::validate_token(token, &get_secret())?;\n    \n    // BUG: This check uses iat instead of exp for staleness\n    if claims.iat < (Utc::now() - Duration::hours(1)).timestamp() as usize {\n        return Err(AuthError::TokenStale);\n    }\n    \n    Ok(next.run(req).await)\n}\n... 60 more lines"}]
    },
    {
      "role": "model",
      "parts": [{"text": "Found it! There are actually TWO bugs:\n1. In `refresh_token()` (jwt.rs line 47): the `iat` field is copied from old claims instead of being set to `now()`\n2. In `auth_middleware` (middleware/auth.rs line 22): the staleness check uses `iat` instead of `exp`, which means refreshed tokens with the stale `iat` always fail\n\nThe fix:\n- jwt.rs: Set `iat: Utc::now().timestamp() as usize` in the refreshed claims\n- middleware.rs: Change the staleness check to use `claims.exp` instead of `claims.iat`"}]
    },
    {
      "role": "user",
      "parts": [{"text": "Good catches! Now apply those fixes and write tests."}]
    }
  ],
  "tools": [
    {
      "function_declarations": [
        {
          "name": "compact_context",
          "description": "The conversation is getting long. Call this tool to produce a compact summary of the conversation so far. Call it alongside your normal response — the summary will replace older messages on the next turn, freeing context space. Focus on preserving: the user's goal, key decisions, file paths, error fixes, and what you were working on most recently.",
          "parameters": {
            "type": "OBJECT",
            "properties": {
              "goal": {
                "type": "STRING",
                "description": "What the user is trying to accomplish."
              },
              "progress": {
                "type": "STRING",
                "description": "What has been done so far."
              },
              "decisions": {
                "type": "ARRAY",
                "items": {"type": "STRING"},
                "description": "Key decisions made and their rationale."
              },
              "active_files": {
                "type": "ARRAY",
                "items": {"type": "STRING"},
                "description": "File paths currently being worked on."
              },
              "next_step": {
                "type": "STRING",
                "description": "The logical next step."
              }
            },
            "required": ["goal", "progress", "next_step"]
          }
        }
      ]
    }
  ],
  "tool_config": {
    "function_calling_config": {
      "mode": "AUTO"
    }
  }
}
PAYLOAD
)

echo "--- Sending request to Gemini ($MODEL) ---"
echo "Conversation: 7 messages (auth bug in Rust project)"
echo "Tool defined: compact_context"
echo ""

# Send request and save response
RESPONSE_FILE="/tmp/piggyback_response.json"
HTTP_CODE=$(curl -s -w "%{http_code}" -o "$RESPONSE_FILE" \
    -X POST \
    -H "Content-Type: application/json" \
    -d "$REQUEST" \
    "$URL")

if [ "$HTTP_CODE" != "200" ]; then
    echo "ERROR: HTTP $HTTP_CODE"
    cat "$RESPONSE_FILE" | python3 -m json.tool 2>/dev/null || cat "$RESPONSE_FILE"
    exit 1
fi

echo "=== Response Analysis ==="
echo ""

# Extract and display results
python3 << 'PYEOF'
import json

with open('/tmp/piggyback_response.json') as f:
    data = json.load(f)

candidates = data.get('candidates', [])
if not candidates:
    print("No candidates returned!")
    sys.exit(1)

parts = candidates[0].get('content', {}).get('parts', [])

text_parts = []
tool_calls = []

for part in parts:
    if 'text' in part:
        text_parts.append(part['text'])
    if 'functionCall' in part:
        tool_calls.append(part['functionCall'])

print(f"--- Response Parts: {len(parts)} total ---")
print(f"  Text parts: {len(text_parts)}")
print(f"  Tool calls: {len(tool_calls)}")
print("")

if text_parts:
    print("--- Normal Response ---")
    for i, t in enumerate(text_parts):
        print(f"  [{i}] {t[:300]}{'...' if len(t) > 300 else ''}")
    print("")

if tool_calls:
    for tc in tool_calls:
        name = tc.get('name', 'unknown')
        print(f"--- Tool Call: {name} ---")
        args = tc.get('args', {})
        for key, val in args.items():
            if isinstance(val, list):
                print(f"  {key}:")
                for item in val:
                    print(f"    - {item}")
            else:
                print(f"  {key}: {val}")
        print("")

    # Check if compact_context was called
    compact_calls = [tc for tc in tool_calls if tc.get('name') == 'compact_context']
    if compact_calls:
        print("=== PIGGYBACK SUCCESS ===")
        args = compact_calls[0].get('args', {})

        # Quality checks
        rendered = json.dumps(args)
        checks = {
            "File paths preserved": any(p in rendered for p in ['src/auth/jwt.rs', 'src/middleware/auth.rs']),
            "Bug details preserved": any(k in rendered.lower() for k in ['iat', 'exp', 'stale', 'refresh']),
            "Goal captured": 'goal' in args and len(args.get('goal', '')) > 10,
            "Next step present": 'next_step' in args and len(args.get('next_step', '')) > 5,
            "Decisions listed": isinstance(args.get('decisions'), list) and len(args.get('decisions', [])) > 0,
            "Active files listed": isinstance(args.get('active_files'), list) and len(args.get('active_files', [])) > 0,
        }

        print("")
        print("--- Quality Checks ---")
        all_pass = True
        for check, passed in checks.items():
            status = "PASS" if passed else "FAIL"
            print(f"  [{status}] {check}")
            if not passed:
                all_pass = False

        print("")
        if all_pass:
            print("All quality checks PASSED - piggyback summary preserves key information!")
        else:
            print("Some quality checks failed - summary may be incomplete.")

        # Estimate token savings
        summary_text = json.dumps(args, indent=2)
        summary_tokens = len(summary_text.split())
        print(f"\n  Summary token estimate: ~{summary_tokens}")
    else:
        print("=== PIGGYBACK: compact_context tool NOT called ===")
        print("The LLM chose not to call the tool. Possible reasons:")
        print("  - Conversation not long enough to trigger compaction")
        print("  - Model chose to respond without tool use")
else:
    print("=== NO TOOL CALLS IN RESPONSE ===")
    print("The LLM responded with text only, no tool calls.")

# Usage metadata
usage = data.get('usageMetadata', {})
if usage:
    print(f"\n--- Token Usage ---")
    print(f"  Prompt tokens: {usage.get('promptTokenCount', '?')}")
    print(f"  Completion tokens: {usage.get('candidatesTokenCount', '?')}")
    print(f"  Total tokens: {usage.get('totalTokenCount', '?')}")
PYEOF

echo ""
echo "=== Experiment Complete ==="
