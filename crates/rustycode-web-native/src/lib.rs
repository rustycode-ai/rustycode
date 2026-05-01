use rustycode_ui_model::*;
use serde_wasm_bindgen::to_value;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct SessionBridge {
    session: FrontendSession,
}

#[wasm_bindgen]
impl SessionBridge {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        Self {
            session: FrontendSession::default(),
        }
    }

    pub fn get_session(&self) -> Result<JsValue, JsValue> {
        to_value(&self.session).map_err(|e| JsValue::from_str(&e.to_string()))
    }

    pub fn submit_input(&mut self, input: String) -> Result<JsValue, JsValue> {
        self.session.input = input;
        let submitted = self.session.submit_input();
        to_value(&submitted).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
