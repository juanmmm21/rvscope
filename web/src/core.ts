import init, { Simulator, models, registers, version, vocabulary } from './wasm/pkg/rv_wasm.js';

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
  readonly title: string;
  readonly equivalent: boolean;
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

export interface Slot {
  readonly pc: number;
  readonly word: number | null;
  readonly text: string | null;
  readonly line: number | null;
  readonly state: 'active' | 'stalled' | 'flushed';
  readonly control: Control | null;
  readonly rs1: Operand | null;
  readonly rs2: Operand | null;
  readonly result: number | null;
  readonly next_pc: number | null;
  readonly address: number | null;
  readonly data_access: DataAccess | null;
  readonly register_write: RegisterWrite | null;
  readonly trap: string | null;
}

export interface StageSnapshot {
  readonly stage: 'single' | 'if' | 'id' | 'ex' | 'mem' | 'wb';
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
}

export interface Core {
  readonly version: string;
  readonly models: readonly ModelInfo[];
  readonly registers: readonly RegisterInfo[];
  readonly vocabulary: Vocabulary;
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
    createMachine: () => new Machine(),
  }));
  return ready;
}
