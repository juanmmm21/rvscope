//! Fachada WebAssembly del simulador: la única pieza que conoce `wasm-bindgen`.

use wasm_bindgen::prelude::wasm_bindgen;

/// Versión del núcleo, para que la interfaz compruebe que ha cargado el wasm correcto.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}
