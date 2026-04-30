# TUI Redesign Specification

**Goal**: Transform RustyCode TUI from "messy and inconsistent" to a polished, professional interface matching Claude Code and Kilocode quality.

---

## 1. Design Principles

### 1.1 Visual Hierarchy
- **Primary content** (conversation) gets most visual weight
- **Secondary info** (status, metadata) is present but muted
- **Tertiary elements** (hints, tips) are subtle

### 1.2 Consistent Structure
- Clear zones with defined boundaries
- Consistent padding and spacing (4-character grid)
- Unified border style throughout

### 1.3 Information Density
- Show what's needed, hide what's not
- Progressive disclosure for advanced info
- No more than 3-4 status indicators visible at once

### 1.4 Visual Restraint
- Maximum 3 colors visible at once (excluding text)
- Use weight (bold/dim) before color for differentiation
- Reserve bright colors for actionable items

---

## 2. Current State Analysis

### 2.1 Current Layout (Problems)
```
┌─────────────────────────────────────────────────────────┐
│Current Session (186 files indexed)                      │ ← Cramped header
│Time    0m 31s                                           │
│Messages 0                                               │
│Status                                                   │
│✓ Ready                                                  │
│                                                         │
│  │ hello, how are you?                                  │
│  │                                                      │
│  │ Hello! I'm doing well...                             │
│  │                                                      │
│  │▏                                                    │ ← Incomplete borders
│                                                         │
│ Ready | 📝 Single-line | 🔧 Code | ☐1                  │ ← Cluttered status
└─────────────────────────────────────────────────────────┘
```

**Issues**:
- Header labels compete for attention (no hierarchy)
- Left-only borders look incomplete/cheap
- Status bar overloaded with mode indicators
- No clear focal point
- Spacing feels arbitrary

### 2.2 Reference: Claude Code Desktop
```
┌─────────────────────────────────────────────────────────┐
│ ⌘-K    rustycode                       ● main    📡 3  │ ← Clean, sparse header
├─────────────────────────────────────────────────────────┤
│                                                         │
│ ╭─ user ──────────────────────────────────────────────╮│
│ │ hello, how are you?                                 ││
│ ╰─────────────────────────────────────────────────────╯│
│                                                         │
│ ╭─ assistant ─────────────────────────────────────────╮│
│ │ Hello! I'm doing well, thank you for asking.       ││
│ │ I'm ready to help you with your coding needs.      ││
│ ╰─────────────────────────────────────────────────────╯│
│                                                         │
│ ╭─ tool ──────────────────────────────────────────────╮│
│ │ ✓ list_dir completed (23 files)                     ││
│ ╰─────────────────────────────────────────────────────╯│
│                                                         │
├─────────────────────────────────────────────────────────┤
│ ▏ Type your message...                    ⏎ Send   ⌘J  │ ← Clean input
└─────────────────────────────────────────────────────────┘
```

### 2.3 Reference: LazyVim Status Line
```
NORMAL  [Git:main]  ● 2  ■ 1  ▲ 0  │  src/main.rs  rust  [LF]  │  45:12  78%
│──────│  │────────│  │────────────────────────────│  │──────│
Mode    Git/LSP    File info                        Position
```
**Key insight**: Sections are clearly separated with dividers, each zone has a single purpose.

---

## 3. Proposed New Design

### 3.1 Layout Zones
```
┌───────────────────────────────────────────────────────────┐
│  HEADER (1 row)                                           │
│  App name | Project | Mode indicators                     │
├───────────────────────────────────────────────────────────┤
│  STATUS BAR (1 row)                                       │
│  LSP/Diagnostics | Git status | Current file context     │
├───────────────────────────────────────────────────────────┤
│                                                           │
│  MESSAGE AREA (flexible, min 10 rows)                    │
│  - User messages (right-aligned bubble style)            │
│  - Assistant messages (full width)                        │
│  - Tool outputs (indented, monospace)                     │
│  - System messages (muted, centered)                      │
│                                                           │
├───────────────────────────────────────────────────────────┤
│  INPUT AREA (3 rows)                                      │
│  - Input field with clear border                          │
│  - Mode indicator (inline, subtle)                        │
│  - Keyboard hints (right-aligned, muted)                 │
├───────────────────────────────────────────────────────────┤
│  FOOTER (1 row)                                           │
│  - Session info | Time | Task count | Model              │
└───────────────────────────────────────────────────────────┘
```

### 3.2 Color Palette
```
Primary (headers, borders):    Blue (#5B8DEF or terminal Blue)
Secondary (subtle UI):         Gray (#666666 or terminal BrightBlack)
Success:                       Green (#4ADE80 or terminal Green)
Warning/Attention:             Yellow (#FACC15 or terminal Yellow)
Error:                         Red (#F87171 or terminal Red)
User accent:                   Cyan (#22D3EE or terminal Cyan)
Assistant accent:              Default white/gray
```

### 3.3 Typography
```
Headers:         Bold, Primary color
User messages:   Bold label, default text
Assistant text:  Regular weight
Tool output:     Monospace font, dimmed
System messages: Italic, dimmed, centered
Status info:     Dimmed, small
Keyboard hints:  Dimmed + underline for keys
```

### 3.4 Spacing System (4-character grid)
```
- Margins: 2 chars from edges
- Padding inside boxes: 1 char
- Between messages: 1 blank line
- Section dividers: 1 line (double or single)
```

---

## 4. Component Specifications

### 4.1 Header Component

**Current**:
```
│Current Session (186 files indexed)
│Time    0m 31s
│Messages 0
```

**New Design**:
```
┌─ rustycode ──── task-manager-app ──── ● 3 tasks ──────────┐
```

**Implementation**:
- Single row, full width
- Left: App name (bold, primary)
- Center: Project name (truncate if needed)
- Right: Active indicators (tasks, pending tools)
- Double line separator below

### 4.2 Status Bar Component

**Current**:
```
│Status
│✓ Ready
```

**New Design**:
```
│ ● LSP ready  │  ✓ main  │  0 problems  │  ☁  synced     │
```

**Implementation**:
- Single row below header
- Sections separated by │ divider
- Each section shows ONE type of info
- Icons for quick visual scan
- Dimmed color, only highlight changes/errors

### 4.3 Message Area Component

**Current**:
```
│ hello, how are you?
│
│ Hello! I'm doing well...
│
│▏
```

**New Design**:
```
╭─ you ────────────────────────────────────────────────────╮
│ hello, how are you?                                      │
╰──────────────────────────────────────────────────────────╯

╭─ assistant ──────────────────────────────────────────────╮
│ Hello! I'm doing well, thank you for asking.            │
│ I'm ready to help you with your coding needs.           │
╰──────────────────────────────────────────────────────────╯

╭─ tool: list_dir ─────────────────────────────────────────╮
│ ✓ Found 23 files (5ms)                                   │
╰──────────────────────────────────────────────────────────╯
```

**Implementation**:
- Full box borders for each message type
- Label shows sender/tool name
- Consistent padding (1 char)
- Blank line between messages
- Tool messages have monospace styling

### 4.4 Input Area Component

**Current**:
```
│ Ready | 📝 Single-line | 🔧 Code | ☐1
│▏
```

**New Design**:
```
┌─ Type a message... ──────────────────────────────────────┐
│                                                          │
│ ▏                                        ⏎ Send   ^J    │
└──────────────────────────────────────────────────────────┘
```

**Implementation**:
- 3 rows total
- Top: Label showing context/mode
- Middle: Input field with cursor
- Right: Primary action + keyboard shortcut
- Mode indicator inline (subtle, right side)

### 4.5 Footer Component

**Current**: (part of status bar, cluttered)
```
│ Ready | 📝 Single-line | 🔧 Code | ☐1
```

**New Design**:
```
│ Session: 2h 34m  │  Tasks: ☐5 ✓3  │  Model: sonnet-4.5  │
```

**Implementation**:
- Single row at bottom
- 3-4 sections maximum
- Session info, task summary, model
- All dimmed (this is reference info, not action)

---

## 5. Implementation Phases

### Phase 1: Foundation (Priority: HIGH) ✅ COMPLETE

1. **Create new theme system** - Define colors, spacing constants ✅
   - Created `crates/rustycode-tui/src/ui/polished_theme.rs`
   - Professional color palette (dark, light, high contrast variants)
   - 4-character grid spacing system
   - Typography helper functions

2. **Implement clean header** - Single row, sparse info ✅
   - Created `crates/rustycode-tui/src/ui/header.rs`
   - Format: `● rustycode ──── project ──── ● 3 tasks`
   - Shows app name, project, git branch, task/tool counts

3. **Implement clean footer** - Move clutter from current status ✅
   - Created `crates/rustycode-tui/src/ui/footer.rs`
   - Format: `Session: 2h 34m │ Tasks: ✓5 ☐3 │ Model: sonnet-4.5`
   - Session duration, task summary, model info

4. **Wire into render flow** ✅
   - Updated `event_loop.rs::render()` with new 5-zone layout
   - Layout: Header(1) | Status(1) | Messages(flex) | Input(3) | Footer(1)
   - Tested via MCPretentious E2E

### Phase 2: Polish (Priority: MEDIUM) ✅ COMPLETE

1. **Implement status bar sections** - Clear divisions ✅
   - Created `crates/rustycode-tui/src/ui/status_bar_polished.rs`
   - Sectioned layout: `│ ● LSP ready │ ✓ main │ 0 problems │ ☁ synced │`
   - Icons for quick visual scan (●, ✓, ⚠, ☁)
   - Color-coded diagnostics (green=0, yellow=1-5, red=6+)

2. **Add input area label** - Mode context ✅
   - Updated `crates/rustycode-tui/src/app/render/input.rs`
   - 3-row input area: Label | Input | Hints
   - Label shows mode: `│ 📝 Single-line` or `│ 📄 Multi-line`
   - Hints show shortcuts: `⏎ Send  Ctrl+J`

3. **Typography pass** - Consistent weights across components ✅
   - Cyan accent for input mode indicators
   - Dark gray for decorative elements
   - Green for primary action (Send)
   - Consistent border styling with │ characters

4. **Animation/smoothing** - Cursor, streaming indicator ✅
   - Blinking cursor (2 FPS toggle)
   - Animator integration for smooth updates

### Phase 3: Advanced (Priority: LOW) ✅ COMPLETE

1. **Collapsible sections** - Hide status/footer on demand ✅
   - Added `status_bar_collapsed` and `footer_collapsed` flags to TUI struct
   - Keyboard shortcut: `Ctrl+Shift+H` toggles both sections
   - Dynamic layout recalculation when toggled
   - System message feedback: "📐 UI sections collapsed/restored"

2. **Custom themes** - Dark, light, high contrast ✅
   - Already implemented in `polished_theme.rs`:
     - `PolishedTheme::default()` - Dark theme (professional blue accent)
     - `PolishedTheme::light()` - Light theme
     - `PolishedTheme::high_contrast()` - High contrast (amber/cyan/magenta)

3. **Accessibility mode** - High contrast, larger text ✅
   - High contrast theme available via `PolishedTheme::high_contrast()`
   - Theme switching infrastructure in place via `ThemeSwitcher`

4. **Responsive layout** - Adapt to small terminals ✅
   - Layout uses `Constraint::Min(0)` for flexible message area
   - Collapsible sections provide extra space when needed
   - Input area adapts height based on available space

---

## 6. Technical Implementation Notes

### 6.1 File Structure
```
crates/rustycode-tui/src/ui/
├── theme.rs          # Color palette, spacing constants
├── header.rs         # New header component
├── status_bar.rs     # New status bar component
├── message_box.rs    # Message bubble rendering
├── input_area.rs     # Input with label and hints
└── footer.rs         # Footer component
```

### 6.2 Key Changes Required
1. `event_loop.rs::render()` - Update layout structure
2. `render/messages.rs` - Box-style message rendering
3. New component files for each zone
4. Update input handler for new input area

### 6.3 Backward Compatibility
- Keep existing components during transition
- Feature flag for new design: `--new-ui`
- Migrate incrementally, component by component

---

## 7. Success Criteria

### Visual
- [ ] No overlapping text at any terminal size
- [ ] Consistent 2-char margins throughout
- [ ] Maximum 3 colors visible simultaneously
- [ ] Clear visual hierarchy (primary > secondary > tertiary)

### Usability
- [ ] User can identify active mode within 1 second
- [ ] Task count visible without searching
- [ ] Current file/project always visible
- [ ] Keyboard hints present but not distracting

### Polish
- [ ] Matches Claude Code visual quality
- [ ] No "cheap" or "incomplete" visual elements
- [ ] Smooth scrolling, no tearing
- [ ] Professional appearance for demos

---

## 8. References

- [Claude Code Internals: Terminal UI](https://kotrotsos.medium.com/claude-code-internals-part-11-terminal-ui-542fe17db016)
- [LazyVim UI Plugins](https://lazyvim.github.io/plugins/ui)
- [OpenClaw UI Overhaul](https://blog.kilo.ai/p/openclaws-ui-just-got-a-quiet-overhaul)
- [Terminal UI Design System](https://github.com/chyinan/terminal-ui-design-system)
