# Web Frontend Replacement Plan

This document defines the strategy for replacing the legacy `rustycode-web` (WASM/ratzilla) frontend with a clean, Rust-native web architecture.

## 1. Goal
Retire the "TUI-in-browser" (ratzilla) approach in favor of a modern, web-native interface that retains RustyCode's core business logic while providing a responsive, accessible, and performant user experience.

## 2. Architectural Principles
*   **Decoupling**: Remove TUI-rendering logic from core business logic.
*   **Platform Neutrality**: Extract session and state management into a pure Rust library with zero TUI dependencies.
*   **Service-Oriented**: The web frontend communicates with the backend/tool-server via standardized JSON-RPC or REST interfaces, isolating it from native Rust constraints.
*   **Legacy Retirement**: The legacy `rustycode-web` crate will be marked for deletion.

## 3. Implementation Phases

### Phase I: Logic Extraction (Foundation)
- Create `rustycode-ui-model` (or similar) to house the platform-agnostic session and state structures (messages, input buffers, request lifecycle).
- Strip `rustycode-ui-core` of TUI rendering dependencies (ratatui, terminal abstractions).
- Ensure `rustycode-ui-model` can be built for both native and `wasm32-unknown-unknown` targets without modification.

### Phase II: Web-Native Interface Development
- Scaffold the new web frontend (e.g., using Dioxus or Leptos, or standard React+WASM).
- Define the new view-to-model adapter layer.
- Implement UI components using native web patterns (CSS/HTML5) instead of box-drawing characters.

### Phase III: Integration and Verification
- Migrate slash command handlers and input parsing to the new architecture.
- Re-verify communication with `rustycode-tool-server`.
- Run parity tests to ensure message flow, tool invocation, and state transitions remain consistent.

### Phase IV: Cleanup
- Remove `crates/rustycode-web` and associated legacy build configs.
- Update project documentation to reflect the new architecture.

## 4. Risks & Mitigations
- **Logic Regressions**: Mitigated by high test coverage in the new `rustycode-ui-model`.
- **UI Parity**: The new interface will intentionally diverge from the TUI aesthetic while retaining the "brutalist" design language via CSS.
- **WASM Constraints**: Continued use of the tool-server proxy remains the standard to handle file/OS restrictions in the browser.

## 5. Success Criteria
- [ ] New web frontend fully functional without `ratzilla` or `ratatui` dependencies.
- [ ] Business logic shared cleanly between native TUI and web via `rustycode-ui-model`.
- [ ] Legacy `crates/rustycode-web` removed from codebase.
- [ ] Zero regression on core request/response lifecycle.
