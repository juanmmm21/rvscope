//! Fachada WebAssembly del simulador: la única pieza que conoce `wasm-bindgen`.
//!
//! Toda la lógica vive en [`session`], que se prueba sin navegador; aquí solo se traducen los
//! argumentos y se serializan las respuestas a objetos de JavaScript. Las formas de esos
//! objetos las describe a mano `web/src/core.ts`: si cambia un `Serialize` de `session`, cambia
//! también allí.

pub mod cheatsheet;
pub mod session;

use serde::Serialize;
use serde_wasm_bindgen::Serializer;
use wasm_bindgen::prelude::{JsError, JsValue, wasm_bindgen};

use crate::session::Session;

/// Versión del núcleo, para que la interfaz compruebe que ha cargado el wasm correcto.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Modelos de procesador disponibles (`ModelInfo[]`).
#[wasm_bindgen]
pub fn models() -> Result<JsValue, JsError> {
    to_js(&Session::models())
}

/// Registros con su nombre ABI, papel y quién los guarda (`RegisterInfo[]`), de la tabla de
/// `rv-isa`: la interfaz no tiene su propia lista.
#[wasm_bindgen]
pub fn registers() -> Result<JsValue, JsError> {
    to_js(&rv_isa::REGISTERS)
}

/// Mnemónicos, pseudoinstrucciones, directivas, registros y modificadores que entiende el
/// ensamblador (`Vocabulary`), para el resaltado del editor.
#[wasm_bindgen]
pub fn vocabulary() -> Result<JsValue, JsError> {
    to_js(&Session::vocabulary())
}

/// Contenido de la chuleta (`CheatSheet`): instrucciones por grupo, pseudoinstrucciones,
/// directivas, ecalls y registros, sacado de las mismas tablas que usa el ensamblador.
#[wasm_bindgen(js_name = cheatSheet)]
pub fn cheat_sheet() -> Result<JsValue, JsError> {
    to_js(&cheatsheet::cheat_sheet())
}

/// `None` se convierte en `null` y los mapas en objetos planos, como en JSON: así los tipos
/// de TypeScript son simples y una instantánea se puede guardar tal cual.
fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    value
        .serialize(&Serializer::json_compatible())
        .map_err(|error| JsError::new(&error.to_string()))
}

fn js_error(error: session::SessionError) -> JsError {
    JsError::new(&error.to_string())
}

#[wasm_bindgen]
pub struct Simulator {
    session: Session,
}

impl Default for Simulator {
    fn default() -> Simulator {
        Simulator::new()
    }
}

#[wasm_bindgen]
impl Simulator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Simulator {
        Simulator {
            session: Session::new(),
        }
    }

    /// Ensambla y carga el programa (`AssembleReport`). Los errores del programa van dentro del
    /// informe; solo falla si el programa no cabe en el mapa de memoria.
    pub fn assemble(&mut self, source: &str) -> Result<JsValue, JsError> {
        let report = self.session.assemble(source).map_err(js_error)?;
        to_js(&report)
    }

    #[wasm_bindgen(js_name = setModel)]
    pub fn set_model(&mut self, id: &str) -> Result<(), JsError> {
        self.session.set_model(id).map_err(js_error)
    }

    pub fn model(&self) -> String {
        self.session.model().id().to_owned()
    }

    pub fn reset(&mut self) -> Result<(), JsError> {
        self.session.reset().map_err(js_error)
    }

    /// Un ciclo (`StepOutcome`).
    pub fn step(&mut self) -> Result<JsValue, JsError> {
        let outcome = self.session.step().map_err(js_error)?;
        to_js(&outcome)
    }

    /// `false` si ya se está al principio.
    #[wasm_bindgen(js_name = stepBack)]
    pub fn step_back(&mut self) -> Result<bool, JsError> {
        self.session.step_back().map_err(js_error)
    }

    /// Ejecuta como mucho `max_cycles` ciclos (`RunStop`).
    pub fn run(&mut self, max_cycles: u32) -> Result<JsValue, JsError> {
        let stop = self.session.run(u64::from(max_cycles)).map_err(js_error)?;
        to_js(&stop)
    }

    #[wasm_bindgen(js_name = setBreakpoints)]
    pub fn set_breakpoints(&mut self, addresses: &[u32]) {
        self.session.set_breakpoints(addresses);
    }

    #[wasm_bindgen(js_name = pushInput)]
    pub fn push_input(&mut self, text: &str) -> Result<(), JsError> {
        self.session.push_input(text).map_err(js_error)
    }

    #[wasm_bindgen(js_name = closeInput)]
    pub fn close_input(&mut self) -> Result<(), JsError> {
        self.session.close_input().map_err(js_error)
    }

    /// Segmentos del mapa de memoria (`Region[]`).
    pub fn regions(&self) -> Result<JsValue, JsError> {
        let regions = self.session.regions().map_err(js_error)?;
        to_js(&regions)
    }

    /// Bytes de memoria; se devuelven como `Uint8Array` sin pasar por serde.
    #[wasm_bindgen(js_name = readMemory)]
    pub fn read_memory(&self, address: u32, len: u32) -> Result<Vec<u8>, JsError> {
        self.session.read_memory(address, len).map_err(js_error)
    }

    /// Estado completo para pintar (`Snapshot`).
    pub fn snapshot(&self) -> Result<JsValue, JsError> {
        let snapshot = self.session.snapshot().map_err(js_error)?;
        to_js(&snapshot)
    }

    /// Tabla del pipeline de los últimos ciclos; se pide aparte de `snapshot` porque solo hace
    /// falta con su panel a la vista.
    #[wasm_bindgen(js_name = pipelineTable)]
    pub fn pipeline_table(&self) -> Result<JsValue, JsError> {
        let table = self.session.pipeline_table().map_err(js_error)?;
        to_js(&table)
    }
}
