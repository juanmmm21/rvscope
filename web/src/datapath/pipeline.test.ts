import { describe, expect, it } from 'vitest';
import type { Control, RegisterInfo, Slot, Snapshot, StageSnapshot, Wires } from '../core';
import {
  PIPELINE_WIRE_IDS,
  describePipeline,
  pipelineState,
  type PipelineStage,
  type PipelineWireId,
} from './pipeline';

const REGISTERS: RegisterInfo[] = ['zero', 'ra', 'sp', 'gp', 'tp', 't0', 't1', 't2', 's0', 's1']
  .concat(['a0', 'a1'])
  .map((abi, index) => ({ index, name: `x${String(index)}`, abi, role: '', saver: 'none' }));

const NONE: Control = {
  operation: null,
  reads_rs1: false,
  reads_rs2: false,
  alu_source: null,
  mem_read: false,
  mem_write: false,
  branch: false,
  jump: false,
  write_source: null,
};

const LOAD: Control = {
  ...NONE,
  reads_rs1: true,
  alu_source: 'immediate',
  mem_read: true,
  write_source: 'memory',
};
const STORE: Control = { ...NONE, reads_rs1: true, reads_rs2: true, mem_write: true };
const ADDI: Control = { ...NONE, reads_rs1: true, alu_source: 'immediate', write_source: 'alu' };
const AUIPC: Control = { ...NONE, alu_source: 'immediate', write_source: 'alu' };
const JALR: Control = {
  ...NONE,
  reads_rs1: true,
  alu_source: 'immediate',
  jump: true,
  write_source: 'link',
};
const BRANCH: Control = { ...NONE, reads_rs1: true, reads_rs2: true, branch: true };

function slot(overrides: Partial<Slot>): Slot {
  return {
    pc: 0x0,
    word: 0x13,
    text: 'nop',
    line: 1,
    state: 'active',
    control: null,
    rs1: null,
    rs2: null,
    result: null,
    next_pc: null,
    address: null,
    data_access: null,
    register_write: null,
    trap: null,
    wires: null,
    ...overrides,
  };
}

function wires(pc: number, overrides: Partial<Wires> = {}): Wires {
  return {
    pc_plus_4: pc + 4,
    immediate: null,
    alu: null,
    target: null,
    pc_source: null,
    ...overrides,
  };
}

function snapshot(slots: Partial<Record<PipelineStage, Slot | null>>, cycle = 6): Snapshot {
  const stages: StageSnapshot[] = (['if', 'id', 'ex', 'mem', 'wb'] as const)
    .filter((stage) => stage in slots)
    .map((stage) => ({ stage, name: stage.toUpperCase(), lanes: [slots[stage] ?? null] }));
  return {
    model: '5-etapas-forwarding',
    status: 'ready',
    pc: 0,
    registers: [],
    changed: [],
    stats: { cycles: cycle, retired: 0, stalls: 0, flushed: 0, dual_issues: 0, cpi: null },
    halt: null,
    output: '',
    pending_input: 0,
    input_closed: false,
    can_step_back: true,
    cycle,
    stages,
  };
}

function active(state: ReturnType<typeof pipelineState>): PipelineWireId[] {
  return PIPELINE_WIRE_IDS.filter((id) => state.wires[id].active);
}

// Ciclo 6 del factorial con forwarding: `jalr` salta desde EX con `ra` adelantado de EX/MEM, IF e
// ID se vacían y WB escribe en a0 lo que leyó `lw`.
const REDIRECT = snapshot({
  if: slot({ pc: 0x0c, text: 'ecall', state: 'flushed', wires: wires(0x0c) }),
  id: slot({
    pc: 0x08,
    text: 'addi a7, zero, 10',
    state: 'flushed',
    control: ADDI,
    rs1: { reg: 0, value: 0, bypass: null },
    wires: wires(0x08, { immediate: 10 }),
  }),
  ex: slot({
    pc: 0x14,
    text: 'jalr ra, 16(ra)',
    control: JALR,
    rs1: { reg: 1, value: 8, bypass: 'ExMem' },
    result: 0x18,
    next_pc: 0x18,
    wires: wires(0x14, {
      immediate: 16,
      alu: { function: 'add', a_source: 'register', b_source: 'immediate', a: 8, b: 16, out: 24 },
      pc_source: 'alu',
    }),
  }),
  mem: slot({
    pc: 0x10,
    text: 'auipc ra, 0x0',
    control: AUIPC,
    result: 8,
    next_pc: 0x14,
    wires: wires(0x10, { immediate: 0 }),
  }),
  wb: slot({
    pc: 0x04,
    text: 'lw a0, 0(a0)',
    control: LOAD,
    rs1: { reg: 10, value: 0x10000000, bypass: null },
    result: 6,
    next_pc: 0x08,
    data_access: { kind: 'load', address: 0x10000000, width: 'word', value: 6 },
    register_write: { reg: 10, value: 6 },
    wires: wires(0x04, { immediate: 0 }),
  }),
});

describe('pipelineState', () => {
  it('antes del primer ciclo no enciende nada', () => {
    const state = pipelineState(snapshot({ if: null, id: null }, 0), REGISTERS);
    expect(active(state)).toEqual([]);
    expect(state.stages.map((card) => card.condition)).toEqual([
      'empty',
      'empty',
      'empty',
      'empty',
      'empty',
    ]);
    expect(describePipeline(state)).toBe('Todavía no se ha ejecutado ningún ciclo.');
    expect(describePipeline(pipelineState(null, REGISTERS))).toBe(
      'Todavía no se ha ejecutado ningún ciclo.',
    );
  });

  it('un salto en EX lleva su destino al PC y apaga lo vaciado', () => {
    const state = pipelineState(REDIRECT, REGISTERS);
    expect(state.stages.map((card) => card.condition)).toEqual([
      'flushed',
      'flushed',
      'active',
      'active',
      'active',
    ]);
    expect(state.muxes.pc).toBe(2);
    expect(state.wires.next_pc).toEqual({ active: true, value: 0x18 });
    expect(state.wires.ex_alu_to_pc).toEqual({ active: true, value: 0x18 });
    expect(state.wires.if_sequential.active).toBe(false);
    for (const id of ['if_pc', 'if_instruction', 'id_instruction', 'id_rs1'] as const) {
      expect(state.wires[id].active).toBe(false);
    }
    // El destino de `jalr` sale de la ALU pero no sigue hacia MEM; sí el enlace pc + 4.
    expect(state.wires.ex_alu_out.active).toBe(false);
    expect(state.wires.ex_pc_plus_4.active).toBe(true);
    expect(state.wires.ex_rd.active).toBe(true);
    // Adelantado desde EX/MEM: el cable de ID/EX no se usa.
    expect(state.muxes.forward_a).toBe(1);
    expect(state.wires.forward_mem_a.active).toBe(true);
    expect(state.wires.ex_rs1.active).toBe(false);
    expect(state.wires.ex_operand_a).toEqual({ active: true, value: 8 });
    expect(state.muxes.forward_b).toBeNull();
    expect(state.muxes.alu_b).toBe(1);
    expect(state.wires.ex_immediate).toEqual({ active: true, value: 16 });
    expect(state.bypasses).toEqual({ a: 'ExMem', b: null });
    // MEM: auipc solo lleva su resultado hacia WB.
    expect(state.wires.mem_alu_result).toEqual({ active: true, value: 8 });
    expect(state.wires.mem_address.active).toBe(false);
    expect(state.memory).toBeNull();
    // WB: lo leído de memoria va a a0.
    expect(state.muxes.write_back).toBe(2);
    expect(state.wires.wb_memory.active).toBe(true);
    expect(state.wires.wb_write_back).toEqual({ active: true, value: 6 });
    expect(state.registers.rd).toBe('a0');
    expect(state.written).toBe(6);
    expect(state.alu).toEqual({ label: 'ADD', out: 24 });
  });

  it('cuenta el ciclo con palabras', () => {
    expect(describePipeline(pipelineState(REDIRECT, REGISTERS))).toBe(
      'Último ciclo: IF ecall (vaciada) · ID addi a7, zero, 10 (vaciada) · EX jalr ra, 16(ra)' +
        ' · MEM auipc ra, 0x0 · WB lw a0, 0(a0); operando A adelantado desde EX/MEM; a0 = 6.',
    );
  });

  it('sin salto, el PC sigue con pc + 4 de la búsqueda', () => {
    const state = pipelineState(
      snapshot({
        if: slot({ pc: 0x2c, text: 'bge t0, a0, 0x44', wires: wires(0x2c) }),
        id: slot({
          pc: 0x28,
          text: 'addi t0, zero, 1',
          control: ADDI,
          rs1: { reg: 0, value: 0, bypass: null },
          wires: wires(0x28, { immediate: 1 }),
        }),
        ex: slot({
          pc: 0x24,
          text: 'sw a0, 0(sp)',
          control: STORE,
          rs1: { reg: 2, value: 0x7fffffe8, bypass: 'MemWb' },
          rs2: { reg: 10, value: 6, bypass: null },
          next_pc: 0x28,
          wires: wires(0x24, {
            immediate: 0,
            alu: {
              function: 'add',
              a_source: 'register',
              b_source: 'immediate',
              a: 0x7fffffe8,
              b: 0,
              out: 0x7fffffe8,
            },
            pc_source: 'sequential',
          }),
        }),
        mem: slot({
          pc: 0x20,
          text: 'sw ra, 4(sp)',
          control: STORE,
          data_access: { kind: 'store', address: 0x7fffffec, width: 'word', value: 0x18 },
          wires: wires(0x20, { immediate: 4 }),
        }),
        wb: null,
      }),
      REGISTERS,
    );
    expect(state.muxes.pc).toBe(0);
    expect(state.wires.if_sequential.active).toBe(true);
    expect(state.wires.next_pc).toEqual({ active: true, value: 0x30 });
    expect(state.pc).toBe(0x2c);
    // ID lee rs1 = zero y genera el inmediato.
    expect(state.registers.rs1).toBe('zero');
    expect(state.wires.id_immediate).toEqual({ active: true, value: 1 });
    expect(state.signals).toEqual(['rd ← ALU']);
    // EX: la dirección usa sp adelantado de MEM/WB y el dato a guardar viene de ID/EX.
    expect(state.muxes.forward_a).toBe(2);
    expect(state.muxes.forward_b).toBe(0);
    expect(state.wires.forward_wb_a.active).toBe(true);
    expect(state.wires.ex_rs2).toEqual({ active: true, value: 6 });
    expect(state.wires.ex_operand_b.active).toBe(false);
    expect(state.wires.ex_store_data).toEqual({ active: true, value: 6 });
    expect(state.wires.ex_alu_out).toEqual({ active: true, value: 0x7fffffe8 });
    // MEM escribe y WB está vacía.
    expect(state.memory).toEqual({ kind: 'store', address: 0x7fffffec, value: 0x18 });
    expect(state.wires.mem_store_data).toEqual({ active: true, value: 0x18 });
    expect(state.wires.mem_read.active).toBe(false);
    expect(state.stages[4]?.condition).toBe('bubble');
    expect(state.muxes.write_back).toBeNull();
    expect(describePipeline(state)).toContain('WB burbuja; operando A adelantado desde MEM/WB.');
  });

  it('con ID retenida, el PC no se escribe', () => {
    const state = pipelineState(
      snapshot({
        if: slot({ pc: 0x08, text: 'auipc ra, 0x0', state: 'stalled', wires: wires(0x08) }),
        id: slot({
          pc: 0x04,
          text: 'lw a0, 0(a0)',
          state: 'stalled',
          control: LOAD,
          rs1: { reg: 10, value: 0, bypass: null },
          wires: wires(0x04, { immediate: 0 }),
        }),
        ex: null,
        mem: null,
        wb: null,
      }),
      REGISTERS,
    );
    expect(state.muxes.pc).toBeNull();
    expect(state.wires.next_pc.active).toBe(false);
    // Lo retenido sigue en sus etapas: IF busca otra vez y ID vuelve a leer a0.
    expect(state.wires.if_pc.active).toBe(true);
    expect(state.wires.id_rs1).toEqual({ active: true, value: 0 });
    expect(state.stages.map((card) => card.condition)).toEqual([
      'stalled',
      'stalled',
      'bubble',
      'bubble',
      'bubble',
    ]);
    expect(describePipeline(state)).toBe(
      'Último ciclo: IF auipc ra, 0x0 (retenida) · ID lw a0, 0(a0) (retenida) · EX burbuja' +
        ' · MEM burbuja · WB burbuja.',
    );
  });

  it('un salto condicional no tomado compara en la ALU y deja el destino sin usar', () => {
    const state = pipelineState(
      snapshot({
        ex: slot({
          pc: 0x2c,
          text: 'bge t0, a0, 0x44',
          control: BRANCH,
          rs1: { reg: 5, value: 1, bypass: 'ExMem' },
          rs2: { reg: 10, value: 6, bypass: null },
          next_pc: 0x30,
          wires: wires(0x2c, {
            immediate: 0x1c,
            alu: { function: 'ge', a_source: 'register', b_source: 'register', a: 1, b: 6, out: 0 },
            target: 0x48,
            pc_source: 'sequential',
          }),
        }),
      }),
      REGISTERS,
    );
    expect(state.wires.ex_target).toEqual({ active: true, value: 0x48 });
    expect(state.wires.ex_immediate_to_target.active).toBe(true);
    expect(state.wires.ex_immediate.active).toBe(false);
    expect(state.wires.ex_operand_b.active).toBe(true);
    expect(state.wires.ex_alu_out.active).toBe(false);
    expect(state.alu).toEqual({ label: '≥', out: 0 });
    // No hay IF que dé pc + 4 en esta instantánea recortada: el PC no se escribe.
    expect(state.muxes.pc).toBeNull();
  });

  it('parado, solo queda WB', () => {
    const state = pipelineState(
      snapshot({
        wb: slot({
          pc: 0x0c,
          text: 'ecall',
          control: NONE,
          trap: 'el programa terminó con código 0',
          wires: wires(0x0c),
        }),
      }),
      REGISTERS,
    );
    expect(state.stages.map((card) => card.condition)).toEqual([
      'empty',
      'empty',
      'empty',
      'empty',
      'active',
    ]);
    expect(active(state)).toEqual([]);
    expect(describePipeline(state)).toBe(
      'Último ciclo: WB ecall; el programa terminó con código 0.',
    );
  });
});
