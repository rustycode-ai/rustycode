# RustyCode TUI Tutorial

Welcome to RustyCode's Terminal User Interface! This tutorial will guide you through everything you need to know to be productive with the TUI.

## Table of Contents

1. [Getting Started](#getting-started)
2. [Basic Navigation](#basic-navigation)
3. [Your First Session](#your-first-session)
4. [Working with Files](#working-with-files)
5. [Editing Code](#editing-code)
6. [Managing Sessions](#managing-sessions)
7. [Advanced Features](#advanced-features)
8. [Tips and Tricks](#tips-and-tricks)
9. [Troubleshooting](#troubleshooting)

---

## Getting Started

### Installation

First, ensure you have RustyCode built:

```bash
# Clone the repository
git clone https://github.com/luengnat/rustycode.git
cd rustycode

# Build in release mode for best performance
cargo build --release
```

### Launching the TUI

```bash
# Using cargo
cargo run --bin rustycode-cli -- tui

# Or the built binary
./target/release/rustycode-cli tui
```

### First-Time Setup

When you first launch the TUI, you'll need to configure your LLM provider:

1. Press `Ctrl+I` to open the provider configuration
2. Use arrow keys to navigate providers
3. Press `c` to configure a provider
4. Enter your API key when prompted
5. Press Enter to save

**Supported Providers:**
- Anthropic (Claude)
- OpenAI (GPT-4, GPT-3.5)
- Google (Gemini)
- Local LLMs (Ollama, LM Studio)

---

## Basic Navigation

### The Interface

The TUI is divided into three main sections:

```
┌─ RustyCode ──────────────────────────────── main ─ My Session ─┐
│ Tools        │ Message transcript                              │
│ ──────────── │                                                 │
│ read_file    │ [you] Explain this code                         │
│ write_file   │                                                 │
│ list_dir     │ [assistant] This code does...                   │
│ bash         │                                                 │
│              │                                                 │
│              │                                                 │
├──────────────┴─────────────────────────────────────────────────┤
│ > Type your message here...                                    │
├────────────────────────────────────────────────────────────────┤
│ i:input  t:tools  q:quit  ↑↓:scroll  Enter:run                │
└────────────────────────────────────────────────────────────────┘
```

**Left Panel (Tools):** Shows available tools and their status
**Main Panel (Chat):** Displays conversation with the AI
**Bottom Panel (Input):** Where you type messages and commands

### Keyboard Basics

- `↑/↓` - Navigate through message history or scroll in lists
- `Enter` - Send message or select item
- `Esc` - Cancel dialogs, stop streaming, or close popups
- `Ctrl+C` - Exit the TUI
- `?` - Show help screen with all shortcuts

### Basic Commands

Type these in the input area (prefix with `/`):

- `/help` - Show detailed help
- `/exit` - Exit the TUI
- `/clear` - Clear current conversation
- `/rename <name>` - Rename current session

---

## Your First Session

### Step 1: Start a Conversation

Let's ask RustyCode to help with a simple task:

```
> Help me create a Rust function that calculates fibonacci numbers
```

The AI will respond with code and explanations.

### Step 2: View the Response

The response will appear in the main panel with:
- Syntax-highlighted code blocks
- Formatted markdown
- Clear explanations

**Navigation:**
- Use `↑/↓` to scroll through long responses
- Press `x` or `Space` to collapse/expand long messages
- Press `Shift+↑/↓` to scroll within expanded messages

### Step 3: Regenerate (Optional)

If you want a different response:

```
Press Ctrl+R
```

This regenerates the response to your last message, maintaining conversation context.

### Step 4: Copy to Clipboard

To copy the last AI response:

```
Press Ctrl+Shift+C
```

The response is now in your system clipboard.

---

## Working with Files

### Opening the File Finder

```
Press Ctrl+F
```

The file finder appears with a search prompt.

### Finding Files

1. Type part of the filename (fuzzy search)
2. Use `↑/↓` to navigate results
3. Press `Enter` to open a file

**Example:** Type "main" to find:
- `src/main.rs`
- `main.py`
- `tests/main_test.rs`

### Code Panel

When you open a file, the code panel appears in a 60/40 split view:

```
┌─ RustyCode ──────────────────────────────── main ─ My Session ─┐
│ Chat         │ Code Panel (src/main.rs)                      │
│ 60%          │ 40%                                            │
│              │                                                 │
│ [your msg]   │ fn main() {                                    │
│              │     println!("Hello");                         │
│ [AI response]│ }                                              │
│              │                                                 │
├──────────────┴─────────────────────────────────────────────────┤
│ > _                                                            │
└────────────────────────────────────────────────────────────────┘
```

**Code Panel Features:**
- Syntax highlighting for 20+ languages
- Line numbers
- Scrollable content
- File name and language in header

### Toggle Code Panel

```
Press Ctrl+O
```

This opens/closes the code panel, giving you more space for chat when closed.

---

## Editing Code

### The Edit Workflow

1. **Open a file** in the code panel (Ctrl+F)
2. **Request an edit** from the AI:
   ```
   > /edit src/main.rs "Add error handling to the main function"
   ```
3. **Review the diff** in edit preview mode
4. **Accept or reject** the changes

### Edit Preview Mode

The diff shows:
- **Green text** - Added lines
- **Red text** - Removed lines
- **Line numbers** - For reference

```
--- src/main.rs (original)
+++ src/main.rs (modified)
@@ -1,3 +1,7 @@
 fn main() {
-    println!("Hello");
+    println!("Hello, World!");
+    if let Err(e) = result {
+        eprintln!("Error: {}", e);
+    }
 }
```

### Accepting/Rejecting Changes

- `Enter` - Accept changes and write to file
- `Esc` - Reject changes and cancel edit

### Manual Edit Command

You can also specify the exact content:

```
> /edit src/main.rs "fn new_function() { println!(\"New\"); }"
```

---

## Managing Sessions

### Session Naming

Give your session a meaningful name:

```
> /rename "Building REST API"
```

The name appears in the header and persists across saves.

### Session History

View all your previous sessions:

```
Press Ctrl+H
```

The session history shows:
- Session names
- Message counts
- Timestamps

**Load a session:**
1. Navigate with `↑/↓`
2. Press `Enter` to load
3. The conversation is restored

### Session Persistence

- Sessions are **auto-saved** on exit
- Session names, messages, and context are preserved
- Load previous sessions anytime with `Ctrl+H`

### Starting a New Session

```
> /clear
```

This clears the current conversation and starts fresh (previous session is saved).

---

## Advanced Features

### Model Selection

**Open Model Selector:**
```
Press Ctrl+M
```

**Quick Switch Shortcuts:**
- `Ctrl+1` - Claude Sonnet 4
- `Ctrl+2` - Claude Opus 4
- `Ctrl+3` - Claude Haiku 4
- `Ctrl+4` - GPT-4o

**When to Use Each Model:**
- **Sonnet 4** - Best balance of speed and quality (default)
- **Opus 4** - Complex reasoning and research
- **Haiku 4** - Quick tasks and simple questions
- **GPT-4o** - Alternative perspective

### Command Palette

Access all commands quickly:

```
Press Ctrl+P
```

Then:
1. Type to filter commands
2. Press `Enter` to execute

**Available Commands:**
- File operations (read, write, list)
- Git operations (status, diff, log)
- Search operations (grep, glob)
- System operations (bash, pwd)

### Provider Management

Configure LLM providers:

```
Press Ctrl+I
```

**Features:**
- Switch between providers
- Update API keys
- Configure model parameters
- Test connection

### Theme Toggle

Switch between dark and light themes:

```
Press Ctrl+T
```

**Dark Theme (Default):**
- Optimized for long coding sessions
- High contrast for code readability

**Light Theme:**
- Better for bright environments
- Maintains color semantics

### Stop Button

Cancel a streaming response:

```
Press Esc
```

**Use Cases:**
- Response is going in the wrong direction
- You got the answer you needed
- You want to rephrase your question

**Result:**
- Streaming stops immediately
- Partial response is preserved
- You can continue the conversation

---

## Tips and Tricks

### 1. Use Descriptive Session Names

```
> /rename "Fixing authentication bug"
> /rename "Adding rate limiting"
> /rename "Database migration"
```

This makes it easy to find specific sessions later.

### 2. Leverage Regenerate

If the first response isn't perfect:
```
Press Ctrl+R
```

Get a different perspective or more detailed explanation.

### 3. Use Code Panel for Context

Keep relevant files open while chatting:
```
1. Ctrl+F → open main.rs
2. Ask questions about the code
3. Keep file open for reference
```

### 4. Chain Edits

Make incremental changes:
```
> /edit src/main.rs "Add error handling"
[Review diff, press Enter]
> /edit src/main.rs "Add logging"
[Review diff, press Enter]
> /edit src/main.rs "Add tests"
[Review diff, press Enter]
```

### 5. Use Session History for Reference

```
1. Press Ctrl+H
2. Find a past session on a similar topic
3. Press Enter to load
4. Review what you learned
```

### 6. Quick Model Switching

Need faster responses?
```
Press Ctrl+3 (Haiku)
```

Need deeper reasoning?
```
Press Ctrl+2 (Opus)
```

### 7. Stop and Redirect

Response not helpful?
```
Press Esc (stop)
> Actually, let's focus on X instead
```

### 8. Use tmux for Persistent Sessions

```bash
# Create a tmux session
tmux new-session -s rustycode 'cargo run --bin rustycode-cli -- tui'

# Detach: Ctrl+B, D
# Attach: tmux attach-session -t rustycode
```

Your TUI state persists even if you close the terminal.

---

## Common Workflows

### Code Review Workflow

```
1. Ctrl+F → open file to review
2. > /review this code for bugs and improvements
3. Review suggestions
4. > /edit src/file.rs [apply specific suggestion]
5. Review diff
6. Press Enter to accept
```

### Debugging Workflow

```
1. > /help debug this error: [paste error]
2. Review AI's diagnosis
3. > /edit src/file.rs [apply fix]
4. Review diff
5. Press Enter to accept
6. > /run tests to verify
```

### Learning Workflow

```
1. > /explain how Rust's ownership works
2. If unclear, press Ctrl+R (regenerate)
3. > /show me a practical example
4. > /explain this line by line
5. Ctrl+Shift+C (copy for notes)
```

### Refactoring Workflow

```
1. > /rename "Refactoring user module"
2. Ctrl+F → open user.rs
3. > /suggest refactoring opportunities
4. Review suggestions
5. > /edit src/user.rs [apply refactor]
6. Review diff, accept or iterate
```

---

## Troubleshooting

### TUI Won't Launch

**Problem:** Terminal doesn't support TUI
**Solution:**
```bash
# Check terminal capabilities
echo $TERM

# Use a modern terminal
# iTerm2, Terminal.app, GNOME Terminal, etc.
```

### Colors Look Wrong

**Problem:** Theme not displaying correctly
**Solution:**
```
Press Ctrl+T (toggle theme)
```

### Session Not Saving

**Problem:** Changes lost on exit
**Solution:**
- Ensure you have write permissions
- Check disk space
- Use `/rename` to ensure session is tracked

### File Finder Can't Find Files

**Problem:** Search returns no results
**Solution:**
- Check you're in the correct directory
- Use more specific search terms
- Ensure file exists: `Ctrl+P` → `list_dir`

### Edit Changes Not Applied

**Problem:** Diff preview shows but file unchanged
**Solution:**
- Ensure you pressed `Enter` (not `Esc`)
- Check file permissions
- Verify file path is correct

### Can't Stop Streaming

**Problem:** `Esc` not stopping response
**Solution:**
- Wait for current token to finish
- If stuck, try `Ctrl+C` to restart TUI
- Check network connectivity

### Performance Issues

**Problem:** TUI is slow or laggy
**Solutions:**
- Use lighter model: `Ctrl+3` (Haiku)
- Close code panel: `Ctrl+O`
- Clear old sessions: `Ctrl+H` → delete unwanted
- Use release build: `cargo build --release`

---

## Next Steps

### Explore More

- [TUI Features Documentation](TUI_FEATURES.md) - Complete feature reference
- [Developer Guide](docs/developer-guide.md) - Contributing to RustyCode
- [API Reference](docs/api-reference.md) - Extending RustyCode

### Practice Exercises

1. **Hello World:** Create a new Rust project with RustyCode
2. **Code Review:** Use the TUI to review and improve existing code
3. **Debug Session:** Fix a bug using the edit workflow
4. **Learn Something:** Ask RustyCode to explain a new concept
5. **Refactor:** Improve code structure with AI assistance

### Join the Community

- Report bugs: [GitHub Issues](https://github.com/luengnat/rustycode/issues)
- Feature requests: [GitHub Discussions](https://github.com/luengnat/rustycode/discussions)
- Contributions: See [CONTRIBUTING](../CONTRIBUTING.md)

---

## Keyboard Shortcut Reference

### Global Shortcuts

| Key | Action |
|-----|--------|
| `Ctrl+C` | Exit TUI |
| `Ctrl+T` | Toggle theme |
| `Ctrl+P` | Command palette |
| `?` | Show help |

### Navigation

| Key | Action |
|-----|--------|
| `Ctrl+F` | File finder |
| `Ctrl+H` | Session history |
| `Ctrl+I` | Provider config |
| `Ctrl+M` | Model selector |
| `↑/↓` | Navigate / Scroll |

### Chat & Code

| Key | Action |
|-----|--------|
| `Ctrl+O` | Toggle code panel |
| `Ctrl+E` | Edit preview mode |
| `Ctrl+R` | Regenerate response |
| `Ctrl+1/2/3/4` | Quick model switch |
| `Ctrl+Shift+C` | Copy to clipboard |
| `x/Space` | Expand/collapse messages |

### Actions

| Key | Action |
|-----|--------|
| `Enter` | Send / Accept |
| `Esc` | Cancel / Stop streaming |

---

**Happy coding with RustyCode!** 🚀

If you have questions or need help, press `?` in the TUI or check the documentation.
