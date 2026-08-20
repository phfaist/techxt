//! Minimal W0 binding — replaced at W1.
use wasm_bindgen::prelude::*;

#[derive(serde::Serialize)]
struct Out {
    text: String,
}

#[wasm_bindgen]
pub struct Session {
    converter: techxt::Converter,
}

#[wasm_bindgen]
impl Session {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Session {
        console_error_panic_hook::set_once();
        Session { converter: techxt::Converter::standard() }
    }

    pub fn convert(&mut self, latex: &str, _options: JsValue) -> Result<JsValue, JsValue> {
        let text = match self.converter.latex_to_text(latex) {
            Ok(c) => c.text,
            Err(e) => format!("error: {}", e.message()),
        };
        serde_wasm_bindgen::to_value(&Out { text })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

#[wasm_bindgen]
pub fn techxt_version() -> String {
    env!("TECHXT_VERSION").to_string()
}
