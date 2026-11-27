import init, {
  Simulator,
  cheatSheet,
  models,
  registers,
  version,
  vocabulary,
} from './wasm/pkg/rv_wasm.js';

// Formas de lo que serializa `crates/rv-wasm/src/session.rs`. Se escriben a mano: si cambia un
// `Serialize` de allí, cambia aquí. `None` llega como `null`.

export interface AsmError {
  readonly line: number;
  readonly column: number;
  readonly length: number;
  readonly message: string;
}

export interface ListingLine {
  readonly address: number;
  readonly word: number;
  readonly line: number;
  readonly text: string;
  readonly pseudo: string | null;
}

export interface Label {
  readonly name: string;
  readonly address: number;
}

export interface AssembleReport {
  readonly ok: boolean;
  readonly errors: readonly AsmError[];
  readonly listing: readonly ListingLine[];
  readonly labels: readonly Label[];
}

export interface ModelInfo {
  readonly id: string;
  // Nombre largo y el corto del selector.
  readonly title: string;
  readonly label: string;
  readonly equivalent: boolean;
  readonly forwarding: boolean;
  readonly pipelined: boolean;
  readonly lanes: number;
}

export interface RegisterInfo {
  readonly index: number;
  readonly name: string;
  readonly abi: string;
  readonly role: string;
  readonly saver: 'none' | 'caller' | 'callee';
}

export interface Vocabulary {
  readonly instructions: readonly string[];
  readonly pseudo_instructions: readonly string[];
  readonly directives: readonly string[];
  readonly registers: readonly string[];
  // Sin el `%`.
  readonly modifiers: readonly string[];
}

// Chuleta de instrucciones (`crates/rv-wasm/src/cheatsheet.rs`).
export type InstructionGroupId =
  | 'arithmetic'
  | 'logical'
  | 'shift'
  | 'compare'
  | 'upper'
  | 'load'
  | 'store'
  | 'branch'
  | 'jump'
  | 'multiply'
  | 'divide'
  | 'system';

export type InstructionFormat = 'R' | 'I' | 'S' | 'B' | 'U' | 'J';

export interface CheatInstruction {
  readonly name: string;
  readonly syntax: string;
  readonly summary: string;
  readonly operation: string;
  readonly format: InstructionFormat;
  readonly extension: 'I' | 'M';
}

export interface CheatGroup {
  readonly id: InstructionGroupId;
  readonly title: string;
  readonly instructions: readonly CheatInstruction[];
}

export interface PseudoInfo {
  readonly name: string;
  readonly syntax: string;
  readonly expansion: string;
  readonly summary: string;
  readonly example: string;
}

export interface DirectiveInfo {
  readonly name: string;
  readonly syntax: string;
  readonly summary: string;
  readonly example: string;
}

export interface SyscallInfo {
  readonly number: number;
  readonly name: string;
  readonly kind: 'output' | 'input' | 'exit';
  // Vacíos si el servicio no recibe argumentos o no devuelve nada.
  readonly arguments: string;
  readonly result: string;
  readonly description: string;
}

export interface CheatSheet {
  readonly groups: readonly CheatGroup[];
  readonly pseudo_instructions: readonly PseudoInfo[];
  readonly directives: readonly DirectiveInfo[];
  readonly syscalls: readonly SyscallInfo[];
  readonly registers: readonly RegisterInfo[];
}

export type StepOutcome = 'stepped' | 'awaiting_input' | 'halted';

export type RunStop =
  | { readonly reason: 'halted' }
  | { readonly reason: 'awaiting_input' }
  | { readonly reason: 'breakpoint'; readonly pc: number }
  | { readonly reason: 'budget' };

export type Status = 'ready' | 'awaiting_input' | 'halted';

export interface Stats {
  readonly cycles: number;
  readonly retired: number;
  readonly stalls: number;
  readonly flushed: number;
  readonly dual_issues: number;
  readonly cpi: number | null;
}

export interface HaltInfo {
  readonly pc: number;
  readonly line: number | null;
  readonly kind: 'exited' | 'breakpoint' | 'fault';
  readonly code: number | null;
  readonly message: string;
}

export type Bypass = 'ExMem' | 'MemWb';

export interface Operand {
  readonly reg: number;
  readonly value: number;
  readonly bypass: Bypass | null;
  // Vía de la instrucción que dio el valor adelantado; va con `bypass` (0 si hay una sola vía).
  readonly bypass_lane: number | null;
}

export interface DataAccess {
  readonly kind: 'fetch' | 'load' | 'store';
  readonly address: number;
  readonly width: 'byte' | 'half' | 'word';
  readonly value: number;
}

export interface RegisterWrite {
  readonly reg: number;
  readonly value: number;
}

export interface Control {
  // La operación es un enum con datos de Rust; la ruta de datos (Fase 5) le dará forma.
  readonly operation: unknown;
  readonly reads_rs1: boolean;
  readonly reads_rs2: boolean;
  readonly alu_source: 'register' | 'immediate' | null;
  readonly mem_read: boolean;
  readonly mem_write: boolean;
  readonly branch: boolean;
  readonly jump: boolean;
  readonly write_source: 'alu' | 'memory' | 'link' | null;
}

export type AluFunction =
  | 'add'
  | 'sub'
  | 'sll'
  | 'slt'
  | 'sltu'
  | 'xor'
  | 'srl'
  | 'sra'
  | 'or'
  | 'and'
  | 'mul'
  | 'mulh'
  | 'mulhsu'
  | 'mulhu'
  | 'div'
  | 'divu'
  | 'rem'
  | 'remu'
  | 'eq'
  | 'ne'
  | 'lt'
  | 'ge'
  | 'ltu'
  | 'geu';

// Entradas y salida de la ALU; en una comparación (`eq`…`geu`), `out` es 1 si se salta.
export interface AluWires {
  readonly function: AluFunction;
  readonly a_source: 'register' | 'pc' | 'zero';
  readonly b_source: 'register' | 'immediate';
  readonly a: number;
  readonly b: number;
  readonly out: number;
}

// Valores de los cables de la ruta de datos (`rv_core::Wires`); lo que depende de la ejecución
// es `null` hasta que la instrucción pasa por la ALU.
export interface Wires {
  readonly pc_plus_4: number;
  readonly immediate: number | null;
  readonly alu: AluWires | null;
  readonly target: number | null;
  readonly pc_source: 'sequential' | 'target' | 'alu' | null;
}

export interface Slot {
  readonly pc: number;
  readonly word: number | null;
  readonly text: string | null;
  readonly line: number | null;
  readonly state: SlotState;
  readonly control: Control | null;
  readonly rs1: Operand | null;
  readonly rs2: Operand | null;
  readonly result: number | null;
  readonly next_pc: number | null;
  readonly address: number | null;
  readonly data_access: DataAccess | null;
  readonly register_write: RegisterWrite | null;
  readonly trap: string | null;
  readonly wires: Wires | null;
}

export type StageId = 'single' | 'if' | 'id' | 'ex' | 'mem' | 'wb';

export type SlotState = 'active' | 'stalled' | 'flushed';

export interface StageSnapshot {
  readonly stage: StageId;
  readonly name: string;
  readonly lanes: readonly (Slot | null)[];
}

export interface Snapshot {
  readonly model: string;
  readonly status: Status;
  readonly pc: number;
  readonly registers: readonly number[];
  readonly changed: readonly number[];
  readonly stats: Stats;
  readonly halt: HaltInfo | null;
  readonly output: string;
  readonly pending_input: number;
  readonly input_closed: boolean;
  readonly can_step_back: boolean;
  readonly cycle: number;
  readonly stages: readonly StageSnapshot[];
}

// Operando que llegó adelantado a EX (`rv_core::TraceBypass`).
export interface TraceBypass {
  readonly reg: number;
  readonly from: Bypass;
  readonly lane: number;
}

// Dónde estaba una instrucción en un ciclo (`rv_core::TraceCell`).
export interface TraceCell {
  readonly cycle: number;
  readonly stage: StageId;
  readonly lane: number;
  readonly state: SlotState;
  // `rs1` y `rs2`; solo en EX y solo si llegaron adelantados.
  readonly bypasses: readonly [TraceBypass | null, TraceBypass | null];
  readonly trap: boolean;
}

// Una instrucción dinámica con sus ciclos consecutivos dentro de la ventana.
export interface TableRow {
  readonly pc: number;
  readonly text: string | null;
  readonly line: number | null;
  readonly cells: readonly TraceCell[];
}

// Tabla del pipeline de los últimos ciclos; los dos ciclos son 0 antes del primer paso.
export interface PipelineTable {
  readonly first_cycle: number;
  readonly last_cycle: number;
  readonly rows: readonly TableRow[];
}

export interface Region {
  readonly kind: 'text' | 'data' | 'stack' | 'io';
  readonly base: number;
  readonly end: number;
  readonly permissions: {
    readonly read: boolean;
    readonly write: boolean;
    readonly execute: boolean;
  };
}

// Envoltorio tipado del `Simulator` del wasm: la web solo habla con el núcleo a través de él.
export class Machine {
  readonly #simulator = new Simulator();

  assemble(source: string): AssembleReport {
    return this.#simulator.assemble(source) as AssembleReport;
  }

  setModel(id: string): void {
    this.#simulator.setModel(id);
  }

  get model(): string {
    return this.#simulator.model();
  }

  reset(): void {
    this.#simulator.reset();
  }

  step(): StepOutcome {
    return this.#simulator.step() as StepOutcome;
  }

  stepBack(): boolean {
    return this.#simulator.stepBack();
  }

  run(maxCycles: number): RunStop {
    return this.#simulator.run(maxCycles) as RunStop;
  }

  setBreakpoints(addresses: readonly number[]): void {
    this.#simulator.setBreakpoints(Uint32Array.from(addresses));
  }

  pushInput(text: string): void {
    this.#simulator.pushInput(text);
  }

  closeInput(): void {
    this.#simulator.closeInput();
  }

  regions(): readonly Region[] {
    return this.#simulator.regions() as Region[];
  }

  readMemory(address: number, length: number): Uint8Array {
    return this.#simulator.readMemory(address, length);
  }

  snapshot(): Snapshot {
    return this.#simulator.snapshot() as Snapshot;
  }

  pipelineTable(): PipelineTable {
    return this.#simulator.pipelineTable() as PipelineTable;
  }
}

export interface Core {
  readonly version: string;
  readonly models: readonly ModelInfo[];
  readonly registers: readonly RegisterInfo[];
  readonly vocabulary: Vocabulary;
  readonly cheatSheet: CheatSheet;
  createMachine(): Machine;
}

// El wasm se inicializa una sola vez; las llamadas posteriores reutilizan la misma promesa.
let ready: Promise<Core> | null = null;

export function loadCore(): Promise<Core> {
  ready ??= init().then(() => ({
    version: version(),
    models: models() as ModelInfo[],
    registers: registers() as RegisterInfo[],
    vocabulary: vocabulary() as Vocabulary,
    cheatSheet: cheatSheet() as CheatSheet,
    createMachine: () => new Machine(),
  }));
  return ready;
}
