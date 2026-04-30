use wasm_bindgen::prelude::*;
use web_sys::console;

use ratzilla::ratatui::{
    style::Stylize,
    widgets::Paragraph,
    Terminal,
};
use ratzilla::{DomBackend, WebRenderer};

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    // Install panic hook for better error messages in the console
    console_error_panic_hook::set_once();

    console::log_1(&"ratzilla-wasm: starting".into());

    let backend = DomBackend::new().map_err(|e| JsValue::from_str(&format!("{}", e)))?;
    let terminal = Terminal::new(backend).map_err(|e| JsValue::from_str(&format!("{}", e)))?;

    terminal.draw_web(|frame| {
        frame.render_widget(
            Paragraph::new("OpenRusty - web TUI is running")
                .green()
                .on_black(),
            frame.area(),
        );
    });

    Ok(())
}
