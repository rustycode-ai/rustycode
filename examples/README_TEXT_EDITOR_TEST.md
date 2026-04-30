# Claude Text Editor Tool - End-to-End Test

This example demonstrates forcing Anthropic Claude to use the `text_editor_20250728` tool.

## Prerequisites

1. Set your Anthropic API key:
```bash
export ANTHROPIC_API_KEY=your_api_key_here
```

2. Build the example:
```bash
cargo build --example test_text_editor_e2e
```

## Running the Test

### Option 1: Run directly
```bash
cargo run --example test_text_editor_e2e
```

### Option 2: Run as a test
```bash
# Run the E2E test (requires --ignored flag)
cargo test --example test_text_editor_e2e -- --ignored

# Run with output
cargo test --example test_text_editor_e2e -- --ignored --nocapture
```

## What This Tests

The example sends three prompts to Claude 3.5 Sonnet:

1. **Create File**: Forces Claude to use `text_editor_20250728` with `create` command
   - Prompt: "Create a file called hello.txt with the content 'Hello from Claude!'"

2. **View File**: Forces Claude to use `text_editor_20250728` with `view` command
   - Prompt: "View the contents of hello.txt"

3. **String Replace**: Forces Claude to use `text_editor_20250728` with `str_replace` command
   - Prompt: "In hello.txt, replace 'Hello' with 'Greetings'"

## Expected Output

```
🧪 Claude Text Editor Tool - End-to-End Test
==========================================

✅ Provider created: Anthropic Claude 3.5 Sonnet

📝 Test 1: Creating a file with text editor tool
-----------------------------------------------
📤 Sending request to Anthropic API...
✅ Response received!
📄 Content: [Claude's response with tool use]

📖 Test 2: Viewing a file with text editor tool
----------------------------------------------
📤 Sending request to Anthropic API...
✅ Response received!
📄 Content: [Claude's response showing file contents]

✏️  Test 3: String replacement with text editor tool
--------------------------------------------------
📤 Sending request to Anthropic API...
✅ Response received!
📄 Content: [Claude's response confirming replacement]

🎉 All tests completed!

📊 Summary:
  - Test 1: File creation - ✅
  - Test 2: File viewing - ✅
  - Test 3: String replacement - ✅
```

## Troubleshooting

### API Key Issues
```
Error: ANTHROPIC_API_KEY environment variable not set
```
**Solution**: Export your API key:
```bash
export ANTHROPIC_API_KEY=sk-ant-your-key-here
```

### Network Issues
```
Error: Failed to create stream: connection timeout
```
**Solution**: Check your internet connection and API endpoint accessibility.

### Tool Not Detected
If the tool isn't being called, verify:
1. The tool is registered in `rustycode-tools/src/lib.rs`
2. The tool name matches `text_editor_20250728` exactly
3. Claude has access to the tool in the system prompt

## Integration with TUI

To test the text editor tool in the full TUI context:
 
```bash
# Start RustyCode TUI via CLI
cargo run --bin rustycode-cli -- tui
 
# In the TUI, type:
Create a file called test.txt with "Hello, World!"
 
# Claude should use the text_editor_20250728 tool
```

## API Response Format

When Claude uses the text editor tool, the response will contain structured tool calls:

```json
{
  "type": "tool_use",
  "id": "toolu_01abc123...",
  "name": "text_editor_20250728",
  "input": {
    "command": "create",
    "path": "hello.txt",
    "content": "Hello from Claude!"
  }
}
```

The tool executor then:
1. Parses the tool call
2. Executes the command
3. Returns the result to Claude
4. Claude continues with the next step

## Cost Considerations

Each test call uses Claude 3.5 Sonnet API:
- Input tokens: ~100-200 per request
- Output tokens: ~100-300 per response
- Total: ~3 API calls × ~400 tokens = ~1,200 tokens

Estimated cost: ~$0.002 per test run (as of 2025)

## Next Steps

1. ✅ Test all 5 commands (view, str_replace, create, insert, undo_edit)
2. ⚠️ Test error handling (file not found, permission denied)
3. ⚠️ Test with large files (>1MB)
4. ⚠️ Test concurrent tool usage
5. ⚠️ Test backup system (undo_edit)

## Related Files

- Tool implementation: `crates/rustycode-tools/src/claude_text_editor.rs`
- Tool registration: `crates/rustycode-tools/src/lib.rs`
- Documentation: `CLAUDE_TEXT_EDITOR_TOOL.md`
- Unit tests: `crates/rustycode-tools/src/claude_text_editor.rs` (line 549+)
