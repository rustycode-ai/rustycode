# Web Frontend Replacement Plan

This document defines the strategy for replacing the legacy `rustycode-web` (WASM/ratzilla) frontend with a clean, **WebSocket-based React frontend**.

## 1. Goal
Retire the "TUI-in-browser" (ratzilla) approach in favor of a modern, web-native interface that offloads session state to the backend and communicates via WebSockets, ensuring consistent behavior across all clients.

## 2. Architectural Principles
*   **Thin Client**: The web frontend is a view-only layer; the backend (RustyCode Gateway) manages the `FrontendSession` state.
*   **WebSocket Synchronization**: Bi-directional communication handles input events and state updates (full state sync + incremental deltas).
*   **Service-Oriented**: The web frontend communicates with the backend gateway, which coordinates with tool-execution servers.
*   **Legacy Retirement**: The legacy `rustycode-web` crate is marked for deletion.

## 3. Implementation Phases

### Phase I: Logic Extraction (Complete)
- [x] Extract `rustycode-ui-model` (platform-neutral state).
- [x] Strip TUI dependencies from core logic.

### Phase II: Backend WebSocket Gateway (New)
- Implement WebSocket handler in the backend using `axum`.
- Integrate `rustycode-ui-model` into the server session controller.
- Implement session recovery, heartbeat handling, and protocol message serialization.

### Phase III: Web-Native Interface Development (Updated)
- Scaffold React frontend (Vite/TypeScript).
- Implement WebSocket client using the `WEBSOCKET_PROTOCOL.md` spec.
- Implement React state management (hook-based subscription to state updates).

### Phase IV: Cleanup
- Remove `crates/rustycode-web`.
- Remove any remaining `ratzilla` or terminal-related dependencies from the new frontend path.

## 4. Risks & Mitigations
- **Logic Regressions**: Mitigated by high test coverage in `rustycode-ui-model`.
- **UI Parity**: The new interface will intentionally diverge from the TUI aesthetic while retaining the "brutalist" design language via CSS.
- **Latency**: WebSocket overhead is minimal compared to the complexity of local WASM-state synchronization and persistence.

## 5. Success Criteria
- [ ] New web frontend fully functional using WebSocket synchronization.
- [ ] Business logic shared cleanly between native TUI and server-side state controller via `rustycode-ui-model`.
- [ ] Legacy `crates/rustycode-web` removed from codebase.
- [ ] Zero regression on core request/response lifecycle.
