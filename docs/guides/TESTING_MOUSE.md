# Testing Mouse Selection and Scrolling

## Verification Procedure
1.  **Launch the TUI**: Run `rustycode`.
2.  **Scrolling**: Use your mouse wheel or trackpad scroll gesture. The conversation list should scroll.
3.  **Selection**:
    - Click and drag inside the conversation panel to copy conversation text.
    - Click and drag inside the sidebar to copy sidebar text.
    - No modifier key is needed.
4.  **Copy**: The selection is copied automatically when you release the mouse.
5.  **Paste**: Ensure the copied text lands in your clipboard.

## Why the Complexity?
We enable mouse capture so the app can distinguish scroll events from drag selection and keep the copy behavior panel-aware.
