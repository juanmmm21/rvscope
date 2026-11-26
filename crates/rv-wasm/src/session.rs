//! Sesión de simulación que maneja la interfaz: un programa ensamblado, el modelo elegido, la
//! máquina en marcha y su historia para el paso atrás. No sabe nada de WebAssembly: la fachada
//! de `lib.rs` solo traduce a JavaScript, así que toda la lógica se prueba con `cargo test`.
//!
//! El paso atrás no guarda un deshacer por instrucción: guarda una copia de la máquina cada
//! `interval` pasos y vuelve re-ejecutando desde la copia más cercana (ADR 0001). La ejecución
//! es determinista, así que basta con recordar cuántos pasos se dieron y en qué momento llegó
//! cada entrada de la consola.

use rv_asm::{AsmError, DIRECTIVES, MODIFIERS, Program};
use rv_core::{
    DataAccess, Halt, Machine, MapError, Model, PipelineView, Processor, Region, RegisterWrite,
    Retired, SlotState, Stage, Stats, Trap,
};
use rv_isa::{INSTRUCTIONS, PSEUDO_INSTRUCTIONS, Reg, RegisterStyle};
use serde::Serialize;
use thiserror::Error;

/// Copias que se guardan como mucho, además de la inicial. Al pasarse se descarta una de cada
/// dos y se duplica el intervalo: la memoria queda acotada y volver atrás nunca re-ejecuta más
/// de `interval` pasos, que crece con la longitud de la ejecución.
const MAX_CHECKPOINTS: usize = 128;
const INITIAL_INTERVAL: u64 = 256;

/// Lo más que se puede leer de memoria de una vez: la interfaz pide solo lo que se ve.
pub const MAX_MEMORY_READ: u32 = 64 * 1024;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("no hay ningún programa ensamblado")]
    NotAssembled,
    #[error("no hay un modelo de procesador «{0}»")]
    UnknownModel(String),
    #[error("no se pueden leer {0} bytes de una vez (como mucho {MAX_MEMORY_READ})")]
    ReadTooLarge(u32),
    #[error(transparent)]
    Map(#[from] MapError),
}

/// Resultado de ensamblar: el listado si ha ido bien o todos los errores con su posición.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AssembleReport {
    pub ok: bool,
    pub errors: Vec<AsmError>,
    pub listing: Vec<ListingLine>,
    pub labels: Vec<Label>,
}

/// Una instrucción del programa: el desensamblado que se pone al lado del editor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ListingLine {
    pub address: u32,
    pub word: u32,
    pub line: usize,
    pub text: String,
    /// Pseudoinstrucción de la que sale, si la hay.
    pub pseudo: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Label {
    pub name: String,
    pub address: u32,
}

/// Palabras que reconoce el ensamblador, para que el editor las resalte sin tener su propia
/// lista: todo sale de las tablas de `rv-isa` y `rv-asm`.
#[derive(Clone, Debug, Serialize)]
pub struct Vocabulary {
    pub instructions: Vec<&'static str>,
    pub pseudo_instructions: Vec<&'static str>,
    pub directives: Vec<&'static str>,
    /// Nombres ABI y numéricos (`a0` y `x10`), más `fp`.
    pub registers: Vec<&'static str>,
    /// Sin el `%`.
    pub modifiers: Vec<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub label: &'static str,
    /// `false` en el modelo que no detecta riesgos: con dependencias da otro resultado.
    pub equivalent: bool,
}

/// Qué pasó al pedir un paso.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    /// Avanzó un ciclo (o se paró en él).
    Stepped,
    /// Una `ecall` espera entrada: no avanza hasta que se le dé.
    AwaitingInput,
    /// Ya estaba parado.
    Halted,
}

/// Por qué se detuvo una ejecución continua.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
pub enum RunStop {
    Halted,
    AwaitingInput,
    /// Una instrucción con punto de parada va a empezar: en el de un ciclo es la siguiente en
    /// ejecutarse y en los segmentados la que acaba de entrar en IF.
    Breakpoint {
        pc: u32,
    },
    /// Se gastaron los ciclos pedidos; la interfaz sigue en el siguiente fotograma.
    Budget,
}

/// Todo lo que la interfaz pinta tras cada operación.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Snapshot {
    pub model: &'static str,
    pub status: Status,
    pub pc: u32,
    pub registers: [u32; 32],
    /// Registros escritos por lo que se retiró en el último paso, para resaltarlos.
    pub changed: Vec<u8>,
    pub stats: StatsView,
    pub halt: Option<HaltView>,
    pub output: String,
    pub pending_input: usize,
    pub input_closed: bool,
    pub can_step_back: bool,
    pub cycle: u64,
    pub stages: Vec<StageSnapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ready,
    AwaitingInput,
    Halted,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct StatsView {
    pub cycles: u64,
    pub retired: u64,
    pub stalls: u64,
    pub flushed: u64,
    pub dual_issues: u64,
    pub cpi: Option<f64>,
}

impl From<Stats> for StatsView {
    fn from(stats: Stats) -> StatsView {
        StatsView {
            cycles: stats.cycles,
            retired: stats.retired,
            stalls: stats.stalls,
            flushed: stats.flushed,
            dual_issues: stats.dual_issues,
            cpi: stats.cpi(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HaltKind {
    /// Terminó con la `ecall` de salida.
    Exited,
    /// `ebreak`.
    Breakpoint,
    /// Cualquier error: acceso a memoria, instrucción ilegal, entrada no válida...
    Fault,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HaltView {
    pub pc: u32,
    pub line: Option<usize>,
    pub kind: HaltKind,
    pub code: Option<i32>,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StageSnapshot {
    pub stage: Stage,
    pub name: &'static str,
    /// Un hueco por vía; `None` es una burbuja.
    pub lanes: Vec<Option<SlotSnapshot>>,
}

/// Hueco de una etapa con lo que la interfaz necesita ya resuelto: el desensamblado y la línea
/// de la fuente, para no tener que decodificar nada en JavaScript.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SlotSnapshot {
    pub pc: u32,
    pub word: Option<u32>,
    pub text: Option<String>,
    pub line: Option<usize>,
    pub state: SlotState,
    pub control: Option<rv_core::Control>,
    pub rs1: Option<rv_core::Operand>,
    pub rs2: Option<rv_core::Operand>,
    pub result: Option<u32>,
    pub next_pc: Option<u32>,
    pub address: Option<u32>,
    pub data_access: Option<DataAccess>,
    pub register_write: Option<RegisterWrite>,
    pub trap: Option<String>,
}

/// Entrada que el usuario dio a la consola tras `at` pasos efectivos.
#[derive(Clone, Debug, PartialEq, Eq)]
struct InputEvent {
    at: u64,
    kind: InputKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InputKind {
    Text(String),
    Close,
}

/// Copia de la máquina justo después del paso `steps`, con las `inputs` primeras entradas ya
/// dadas (las del propio instante `steps` se dan después).
#[derive(Clone, Debug)]
struct Checkpoint {
    steps: u64,
    inputs: usize,
    machine: Machine,
}

/// Programa cargado en una máquina, con su historia.
#[derive(Clone, Debug)]
struct Loaded {
    program: Program,
    machine: Machine,
    /// Pasos que han hecho algo: los de espera de entrada y los dados ya parado no cuentan.
    steps: u64,
    inputs: Vec<InputEvent>,
    checkpoints: Vec<Checkpoint>,
    interval: u64,
    last_retired: Vec<Retired>,
    awaiting: bool,
}

impl Loaded {
    fn new(program: Program, model: Model) -> Result<Loaded, MapError> {
        let machine = model.boot(&program)?;
        Ok(Loaded {
            checkpoints: vec![Checkpoint {
                steps: 0,
                inputs: 0,
                machine: machine.clone(),
            }],
            program,
            machine,
            steps: 0,
            inputs: Vec::new(),
            interval: INITIAL_INTERVAL,
            last_retired: Vec::new(),
            awaiting: false,
        })
    }

    fn step(&mut self) -> StepOutcome {
        if self.machine.halt().is_some() {
            return StepOutcome::Halted;
        }
        let retired = self.machine.step();
        // Un ciclo de espera no cambia nada: no se apunta, y así la historia no depende de lo
        // que se tarde en teclear.
        self.awaiting = self.machine.awaiting_input();
        if self.awaiting {
            return StepOutcome::AwaitingInput;
        }
        self.last_retired = retired;
        self.steps += 1;
        if self.steps % self.interval == 0 {
            self.save_checkpoint();
        }
        StepOutcome::Stepped
    }

    fn save_checkpoint(&mut self) {
        self.checkpoints.push(Checkpoint {
            steps: self.steps,
            inputs: self.inputs.len(),
            machine: self.machine.clone(),
        });
        if self.checkpoints.len() > MAX_CHECKPOINTS + 1 {
            self.interval *= 2;
            let interval = self.interval;
            self.checkpoints
                .retain(|checkpoint| checkpoint.steps % interval == 0);
        }
    }

    /// Vuelve al estado de justo antes del último paso. La entrada dada después de ese paso se
    /// conserva (vuelve a la cola), y la que consumió ese paso también: deshacer una lectura
    /// devuelve lo leído a la consola.
    fn step_back(&mut self) -> bool {
        let Some(target) = self.steps.checked_sub(1) else {
            return false;
        };
        self.checkpoints
            .retain(|checkpoint| checkpoint.steps <= target);
        // Se parte de una copia estrictamente anterior para repetir al menos el último paso y
        // saber qué retiró (lo que se resalta); la inicial vale cuando se vuelve al principio.
        let Some(checkpoint) = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.steps < target || checkpoint.steps == 0)
            .cloned()
        else {
            return false;
        };
        for event in &mut self.inputs {
            event.at = event.at.min(target);
        }
        let mut machine = checkpoint.machine;
        let mut next_input = checkpoint.inputs;
        let mut last_retired = Vec::new();
        for steps in checkpoint.steps..target {
            next_input = self.apply_inputs(&mut machine, next_input, steps);
            last_retired = machine.step();
        }
        self.apply_inputs(&mut machine, next_input, target);
        self.machine = machine;
        self.last_retired = last_retired;
        self.steps = target;
        self.awaiting = false;
        true
    }

    /// Da a `machine` las entradas desde `from` cuyo instante es `at`; devuelve la siguiente.
    fn apply_inputs(&self, machine: &mut Machine, from: usize, at: u64) -> usize {
        let mut index = from;
        while let Some(event) = self.inputs.get(index).filter(|event| event.at == at) {
            apply_input(machine, &event.kind);
            index += 1;
        }
        index
    }

    fn give_input(&mut self, kind: InputKind) {
        apply_input(&mut self.machine, &kind);
        self.inputs.push(InputEvent {
            at: self.steps,
            kind,
        });
        self.awaiting = false;
    }

    fn breakpoint_hit(&self, breakpoints: &[u32]) -> Option<u32> {
        if self.machine.halt().is_some() {
            return None;
        }
        match self.machine {
            Machine::SingleCycle(_) => {
                let pc = self.machine.state().pc;
                breakpoints.contains(&pc).then_some(pc)
            }
            // Una instrucción retenida en IF no vuelve a disparar: si no, una burbuja pararía
            // la ejecución en el mismo sitio una y otra vez.
            Machine::FiveStage(_) | Machine::DualIssue(_) => self
                .machine
                .view()
                .stage(Stage::If)
                .into_iter()
                .flat_map(|stage| stage.lanes.iter().flatten())
                .filter(|slot| slot.state != SlotState::Stalled)
                .map(|slot| slot.pc)
                .find(|pc| breakpoints.contains(pc)),
        }
    }
}

fn apply_input(machine: &mut Machine, kind: &InputKind) {
    match kind {
        InputKind::Text(text) => machine.console_mut().push_input(text),
        InputKind::Close => machine.console_mut().close_input(),
    }
}

/// Sesión completa: lo que la web crea una vez y va usando.
#[derive(Clone, Debug)]
pub struct Session {
    model: Model,
    loaded: Option<Loaded>,
    breakpoints: Vec<u32>,
}

impl Default for Session {
    fn default() -> Session {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session {
            model: Model::SingleCycle,
            loaded: None,
            breakpoints: Vec::new(),
        }
    }

    pub fn models() -> Vec<ModelInfo> {
        Model::ALL
            .into_iter()
            .map(|model| ModelInfo {
                id: model.id(),
                title: model.title(),
                label: model.label(),
                equivalent: model.is_equivalent(),
            })
            .collect()
    }

    pub fn vocabulary() -> Vocabulary {
        let mut registers: Vec<&'static str> = (0..32u8)
            .filter_map(Reg::new)
            .flat_map(|reg| [reg.abi_name(), reg.numeric_name()])
            .collect();
        registers.push("fp");
        Vocabulary {
            instructions: INSTRUCTIONS.iter().map(|spec| spec.name).collect(),
            pseudo_instructions: PSEUDO_INSTRUCTIONS.iter().map(|spec| spec.name).collect(),
            directives: DIRECTIVES.iter().map(|spec| spec.name).collect(),
            registers,
            modifiers: MODIFIERS.to_vec(),
        }
    }

    pub fn model(&self) -> Model {
        self.model
    }

    /// Ensambla `source` y, si no hay errores, lo carga en el modelo actual desde el principio.
    /// Con errores se queda el programa anterior, para no perder lo que se estaba mirando.
    pub fn assemble(&mut self, source: &str) -> Result<AssembleReport, SessionError> {
        let program = match rv_asm::assemble(source) {
            Ok(program) => program,
            Err(errors) => {
                return Ok(AssembleReport {
                    ok: false,
                    errors,
                    listing: Vec::new(),
                    labels: Vec::new(),
                });
            }
        };
        let report = AssembleReport {
            ok: true,
            errors: Vec::new(),
            listing: program
                .listing
                .iter()
                .map(|entry| ListingLine {
                    address: entry.address,
                    word: entry.word,
                    line: entry.line,
                    text: entry
                        .instruction
                        .disassemble(entry.address, RegisterStyle::Abi),
                    pseudo: entry.pseudo,
                })
                .collect(),
            labels: program
                .labels
                .iter()
                .map(|(name, &address)| Label {
                    name: name.clone(),
                    address,
                })
                .collect(),
        };
        self.loaded = Some(Loaded::new(program, self.model)?);
        Ok(report)
    }

    /// Cambia de modelo y vuelve a empezar el programa cargado en él.
    pub fn set_model(&mut self, id: &str) -> Result<(), SessionError> {
        let model = Model::from_id(id).ok_or_else(|| SessionError::UnknownModel(id.to_owned()))?;
        self.model = model;
        if let Some(loaded) = self.loaded.take() {
            self.loaded = Some(Loaded::new(loaded.program, model)?);
        }
        Ok(())
    }

    /// Vuelve al principio del programa, sin entrada.
    pub fn reset(&mut self) -> Result<(), SessionError> {
        let loaded = self.loaded.take().ok_or(SessionError::NotAssembled)?;
        self.loaded = Some(Loaded::new(loaded.program, self.model)?);
        Ok(())
    }

    pub fn step(&mut self) -> Result<StepOutcome, SessionError> {
        Ok(self.loaded_mut()?.step())
    }

    /// `false` si ya se está al principio.
    pub fn step_back(&mut self) -> Result<bool, SessionError> {
        Ok(self.loaded_mut()?.step_back())
    }

    /// Ejecuta hasta pararse, esperar entrada, llegar a un punto de parada o gastar
    /// `max_cycles` pasos. Siempre avanza al menos uno, así que reanudar desde un punto de
    /// parada no se queda en el mismo sitio.
    pub fn run(&mut self, max_cycles: u64) -> Result<RunStop, SessionError> {
        let breakpoints = self.breakpoints.clone();
        let loaded = self.loaded_mut()?;
        for _ in 0..max_cycles {
            match loaded.step() {
                StepOutcome::Halted => return Ok(RunStop::Halted),
                StepOutcome::AwaitingInput => return Ok(RunStop::AwaitingInput),
                StepOutcome::Stepped => {}
            }
            if loaded.machine.halt().is_some() {
                return Ok(RunStop::Halted);
            }
            if let Some(pc) = loaded.breakpoint_hit(&breakpoints) {
                return Ok(RunStop::Breakpoint { pc });
            }
        }
        Ok(RunStop::Budget)
    }

    /// Direcciones con punto de parada (la interfaz traduce líneas con el listado).
    pub fn set_breakpoints(&mut self, addresses: &[u32]) {
        let mut addresses = addresses.to_vec();
        addresses.sort_unstable();
        addresses.dedup();
        self.breakpoints = addresses;
    }

    pub fn push_input(&mut self, text: &str) -> Result<(), SessionError> {
        self.loaded_mut()?
            .give_input(InputKind::Text(text.to_owned()));
        Ok(())
    }

    pub fn close_input(&mut self) -> Result<(), SessionError> {
        self.loaded_mut()?.give_input(InputKind::Close);
        Ok(())
    }

    pub fn regions(&self) -> Result<Vec<Region>, SessionError> {
        Ok(self
            .loaded()?
            .machine
            .state()
            .memory
            .map()
            .regions()
            .to_vec())
    }

    /// Bytes de `[address, address + len)`; lo que no está en el mapa se lee como ceros.
    pub fn read_memory(&self, address: u32, len: u32) -> Result<Vec<u8>, SessionError> {
        if len > MAX_MEMORY_READ {
            return Err(SessionError::ReadTooLarge(len));
        }
        Ok(self.loaded()?.machine.state().memory.peek(address, len))
    }

    pub fn snapshot(&self) -> Result<Snapshot, SessionError> {
        let loaded = self.loaded()?;
        let machine = &loaded.machine;
        let state = machine.state();
        let halt = machine.halt();
        let status = if halt.is_some() {
            Status::Halted
        } else if loaded.awaiting {
            Status::AwaitingInput
        } else {
            Status::Ready
        };
        let mut changed: Vec<u8> = loaded
            .last_retired
            .iter()
            .filter_map(|retired| retired.register_write)
            .filter_map(|write| u8::try_from(write.reg.index()).ok())
            .collect();
        changed.sort_unstable();
        changed.dedup();
        let view = machine.view();
        Ok(Snapshot {
            model: self.model.id(),
            status,
            pc: state.pc,
            registers: *state.registers.values(),
            changed,
            stats: machine.stats().into(),
            halt: halt.map(|halt| halt_view(&loaded.program, halt)),
            output: state.console.output_text(),
            pending_input: state.console.pending_input(),
            input_closed: state.console.is_input_closed(),
            can_step_back: loaded.steps > 0,
            cycle: view.cycle,
            stages: stage_snapshots(&loaded.program, view),
        })
    }

    fn loaded(&self) -> Result<&Loaded, SessionError> {
        self.loaded.as_ref().ok_or(SessionError::NotAssembled)
    }

    fn loaded_mut(&mut self) -> Result<&mut Loaded, SessionError> {
        self.loaded.as_mut().ok_or(SessionError::NotAssembled)
    }
}

fn halt_view(program: &Program, halt: Halt) -> HaltView {
    let (kind, code) = match halt.trap {
        Trap::Exited { code } => (HaltKind::Exited, Some(code)),
        Trap::Breakpoint => (HaltKind::Breakpoint, None),
        _ => (HaltKind::Fault, None),
    };
    HaltView {
        pc: halt.pc,
        line: program.line_at(halt.pc),
        kind,
        code,
        message: halt.trap.to_string(),
    }
}

fn stage_snapshots(program: &Program, view: &PipelineView) -> Vec<StageSnapshot> {
    view.stages
        .iter()
        .map(|stage| StageSnapshot {
            stage: stage.stage,
            name: stage.stage.name(),
            lanes: stage
                .lanes
                .iter()
                .map(|lane| {
                    lane.as_ref().map(|slot| SlotSnapshot {
                        pc: slot.pc,
                        word: slot.word,
                        text: slot.instruction.map(|instruction| {
                            instruction.disassemble(slot.pc, RegisterStyle::Abi)
                        }),
                        line: program.line_at(slot.pc),
                        state: slot.state,
                        control: slot.control,
                        rs1: slot.rs1,
                        rs2: slot.rs2,
                        result: slot.result,
                        next_pc: slot.next_pc,
                        address: slot.address,
                        data_access: slot.data_access,
                        register_write: slot.register_write,
                        trap: slot.trap.map(|trap| trap.to_string()),
                    })
                })
                .collect(),
        })
        .collect()
}
