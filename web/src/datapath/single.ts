import type { RegisterInfo, Slot, Snapshot } from '../core';

// Qué enseña el diagrama del procesador de un ciclo para la instrucción que acaba de ejecutar.
// Solo traduce lo que dice el núcleo (señales de control y cables) a qué cable está en uso, qué
// entrada elige cada multiplexor y qué texto lleva cada pieza; no calcula nada del procesador.

export const WIRE_IDS = [
  'next_pc',
  'pc',
  'pc_plus_4',
  'instruction',
  'rs1_index',
  'rs2_index',
  'rd_index',
  'rs1',
  'rs2',
  'store_data',
  'immediate',
  'pc_to_alu',
  'zero',
  'pc_to_target',
  'immediate_to_target',
  'alu_a',
  'alu_b',
  'address',
  'alu_to_write_back',
  'alu_to_pc',
  'target',
  'memory_read',
  'link',
  'write_back',
] as const;

export type WireId = (typeof WIRE_IDS)[number];

export const MUX_IDS = ['pc', 'alu_a', 'alu_b', 'write_back'] as const;

export type MuxId = (typeof MUX_IDS)[number];

export interface WireState {
  readonly active: boolean;
  // Valor que se rotula en el cable, si ese cable lleva rótulo.
  readonly value: number | null;
}

export interface MemoryAccessView {
  readonly kind: 'load' | 'store';
  readonly address: number;
  readonly value: number;
}

export interface SingleCycleState {
  readonly wires: Readonly<Record<WireId, WireState>>;
  // Entrada elegida de cada multiplexor, de arriba abajo; `null` si no se usa.
  readonly muxes: Readonly<Record<MuxId, number | null>>;
  // Cierto si la instrucción llegó a decodificarse (hay señales de control).
  readonly decoded: boolean;
  readonly pc: number | null;
  readonly word: number | null;
  readonly text: string | null;
  readonly signals: readonly string[];
  readonly registers: {
    readonly rs1: string | null;
    readonly rs2: string | null;
    readonly rd: string | null;
  };
  readonly alu: { readonly label: string; readonly out: number } | null;
  readonly memory: MemoryAccessView | null;
  // Valor que se escribe en `rd`.
  readonly written: number | null;
  readonly trap: string | null;
}

const ALU_LABELS: Readonly<Record<string, string>> = {
  eq: '=',
  ne: '≠',
  lt: '<',
  ge: '≥',
  ltu: '<u',
  geu: '≥u',
};

// Un cable que se ramifica (rs2 a la ALU y a la memoria, el inmediato a la ALU y al sumador de
// saltos) se dibuja como cables distintos con el rótulo en el mismo sitio: solo se enciende la
// rama que la instrucción usa de verdad.
const WRITE_SOURCES = { alu: 'ALU', memory: 'memoria', link: 'pc + 4' } as const;

function wire(active: boolean, value: number | null = null): WireState {
  return { active, value: active ? value : null };
}

function idle(): Record<WireId, WireState> {
  const wires = {} as Record<WireId, WireState>;
  for (const id of WIRE_IDS) {
    wires[id] = wire(false);
  }
  return wires;
}

const NO_MUXES: Readonly<Record<MuxId, number | null>> = {
  pc: null,
  alu_a: null,
  alu_b: null,
  write_back: null,
};

function registerName(registers: readonly RegisterInfo[], index: number): string {
  return registers[index]?.abi ?? `x${String(index)}`;
}

// La instrucción del procesador de un ciclo: su única etapa, o nada antes de empezar.
export function singleCycleSlot(snapshot: Snapshot): Slot | null {
  const [stage] = snapshot.stages;
  if (snapshot.stages.length !== 1 || stage?.stage !== 'single') {
    return null;
  }
  return stage.lanes[0] ?? null;
}

export function singleCycleState(
  slot: Slot | null,
  registers: readonly RegisterInfo[],
): SingleCycleState {
  const control = slot?.control ?? null;
  const wires = slot?.wires ?? null;
  const base = {
    pc: slot?.pc ?? null,
    word: slot?.word ?? null,
    text: slot?.text ?? null,
    trap: slot?.trap ?? null,
  };
  if (slot === null || control === null || wires === null) {
    return {
      ...base,
      decoded: false,
      wires: idle(),
      muxes: NO_MUXES,
      signals: [],
      registers: { rs1: null, rs2: null, rd: null },
      alu: null,
      memory: null,
      written: null,
    };
  }

  const alu = wires.alu;
  const write = slot.register_write;
  const access =
    slot.data_access !== null && slot.data_access.kind !== 'fetch'
      ? {
          kind: slot.data_access.kind,
          address: slot.data_access.address,
          value: slot.data_access.value,
        }
      : null;
  const source = write === null ? null : control.write_source;
  const writes = source !== null;
  const executed = slot.next_pc !== null;

  const state: Record<WireId, WireState> = {
    next_pc: wire(executed, slot.next_pc),
    pc: wire(true),
    pc_plus_4: wire(true, wires.pc_plus_4),
    instruction: wire(true),
    rs1_index: wire(control.reads_rs1),
    rs2_index: wire(control.reads_rs2),
    rd_index: wire(writes),
    rs1: wire(control.reads_rs1, slot.rs1?.value ?? null),
    rs2: wire(alu?.b_source === 'register', slot.rs2?.value ?? null),
    store_data: wire(control.mem_write, slot.rs2?.value ?? null),
    immediate: wire(alu?.b_source === 'immediate', wires.immediate),
    pc_to_alu: wire(alu?.a_source === 'pc'),
    zero: wire(alu?.a_source === 'zero'),
    pc_to_target: wire(wires.target !== null),
    immediate_to_target: wire(wires.target !== null, wires.immediate),
    alu_a: wire(alu !== null),
    alu_b: wire(alu !== null),
    address: wire(control.mem_read || control.mem_write),
    alu_to_write_back: wire(writes && source === 'alu'),
    alu_to_pc: wire(wires.pc_source === 'alu'),
    target: wire(wires.target !== null, wires.target),
    memory_read: wire(control.mem_read),
    link: wire(writes && source === 'link'),
    write_back: wire(writes, write?.value ?? null),
  };

  const muxes: Record<MuxId, number | null> = {
    pc: wires.pc_source === null ? null : { sequential: 0, target: 1, alu: 2 }[wires.pc_source],
    alu_a: alu === null ? null : { register: 0, pc: 1, zero: 2 }[alu.a_source],
    alu_b: alu === null ? null : { register: 0, immediate: 1 }[alu.b_source],
    write_back: source === null ? null : { alu: 0, memory: 1, link: 2 }[source],
  };

  const signals: string[] = [];
  if (alu !== null) {
    signals.push(`ALU ${ALU_LABELS[alu.function] ?? alu.function}`);
    if (alu.b_source === 'immediate') {
      signals.push('B ← inmediato');
    }
  }
  if (control.mem_read) {
    signals.push('lee memoria');
  }
  if (control.mem_write) {
    signals.push('escribe memoria');
  }
  if (control.branch) {
    signals.push('salto condicional');
  }
  if (control.jump) {
    signals.push('salto');
  }
  if (source !== null) {
    signals.push(`rd ← ${WRITE_SOURCES[source]}`);
  }

  return {
    ...base,
    decoded: true,
    wires: state,
    muxes,
    signals,
    registers: {
      rs1: slot.rs1 === null ? null : registerName(registers, slot.rs1.reg),
      rs2: slot.rs2 === null ? null : registerName(registers, slot.rs2.reg),
      rd: write === null || source === null ? null : registerName(registers, write.reg),
    },
    alu:
      alu === null
        ? null
        : { label: (ALU_LABELS[alu.function] ?? alu.function).toUpperCase(), out: alu.out },
    memory: access,
    written: write === null || source === null ? null : write.value,
  };
}
