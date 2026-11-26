import { describe, expect, it } from 'vitest';
import type { Control, RegisterInfo, Slot, Snapshot, Wires } from '../core';
import { splitSignals } from './svg';
import { describeCycle } from './panel';
import { WIRE_IDS, singleCycleSlot, singleCycleState, type WireId } from './single';

const REGISTERS: RegisterInfo[] = [
  'zero',
  'ra',
  'sp',
  'gp',
  'tp',
  't0',
  't1',
  't2',
  's0',
  's1',
].map((abi, index) => ({ index, name: `x${String(index)}`, abi, role: '', saver: 'none' }));
REGISTERS.push(
  ...['a0', 'a1'].map((abi, offset) => ({
    index: 10 + offset,
    name: `x${String(10 + offset)}`,
    abi,
    role: '',
    saver: 'none' as const,
  })),
);

const NO_CONTROL: Control = {
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

function slot(overrides: Partial<Slot>): Slot {
  return {
    pc: 0x40,
    word: 0x13,
    text: 'nop',
    line: 1,
    state: 'active',
    control: null,
    rs1: null,
    rs2: null,
    result: null,
    next_pc: 0x44,
    address: null,
    data_access: null,
    register_write: null,
    trap: null,
    wires: null,
    ...overrides,
  };
}

function wires(overrides: Partial<Wires>): Wires {
  return {
    pc_plus_4: 0x44,
    immediate: null,
    alu: null,
    target: null,
    pc_source: 'sequential',
    ...overrides,
  };
}

function active(state: ReturnType<typeof singleCycleState>): WireId[] {
  return WIRE_IDS.filter((id) => state.wires[id].active);
}

const ADDI_CONTROL: Control = {
  ...NO_CONTROL,
  reads_rs1: true,
  alu_source: 'immediate',
  write_source: 'alu',
};

const ADDI = slot({
  text: 'addi a0, a1, -1',
  control: ADDI_CONTROL,
  rs1: { reg: 11, value: 5, bypass: null, bypass_lane: null },
  result: 4,
  register_write: { reg: 10, value: 4 },
  wires: wires({
    immediate: 0xffffffff,
    alu: { function: 'add', a_source: 'register', b_source: 'immediate', a: 5, b: -1, out: 4 },
  }),
});

describe('singleCycleState', () => {
  it('enciende solo el camino de una operación con inmediato', () => {
    const state = singleCycleState(ADDI, REGISTERS);
    expect(active(state)).toEqual([
      'next_pc',
      'pc',
      'pc_plus_4',
      'instruction',
      'rs1_index',
      'rd_index',
      'rs1',
      'immediate',
      'alu_a',
      'alu_b',
      'alu_to_write_back',
      'write_back',
    ]);
    expect(state.muxes).toEqual({ pc: 0, alu_a: 0, alu_b: 1, write_back: 0 });
    expect(state.registers).toEqual({ rs1: 'a1', rs2: null, rd: 'a0' });
    expect(state.wires.write_back.value).toBe(4);
    expect(state.wires.immediate.value).toBe(0xffffffff);
    expect(state.alu).toEqual({ label: 'ADD', out: 4 });
    expect(state.signals).toEqual(['ALU add', 'B ← inmediato', 'rd ← ALU']);
    expect(state.written).toBe(4);
  });

  it('un almacenamiento lleva rs2 a la memoria, no a la ALU', () => {
    const state = singleCycleState(
      slot({
        text: 'sw ra, 4(sp)',
        control: {
          ...NO_CONTROL,
          reads_rs1: true,
          reads_rs2: true,
          alu_source: 'immediate',
          mem_write: true,
        },
        rs1: { reg: 2, value: 0x7ffffff0, bypass: null, bypass_lane: null },
        rs2: { reg: 1, value: 0x10, bypass: null, bypass_lane: null },
        address: 0x7ffffff4,
        data_access: { kind: 'store', address: 0x7ffffff4, width: 'word', value: 0x10 },
        wires: wires({
          immediate: 4,
          alu: {
            function: 'add',
            a_source: 'register',
            b_source: 'immediate',
            a: 0x7ffffff0,
            b: 4,
            out: 0x7ffffff4,
          },
        }),
      }),
      REGISTERS,
    );
    expect(state.wires.rs2.active).toBe(false);
    expect(state.wires.store_data).toEqual({ active: true, value: 0x10 });
    expect(state.wires.address.active).toBe(true);
    expect(state.wires.write_back.active).toBe(false);
    expect(state.muxes.write_back).toBeNull();
    expect(state.memory).toEqual({ kind: 'store', address: 0x7ffffff4, value: 0x10 });
    expect(state.registers).toEqual({ rs1: 'sp', rs2: 'ra', rd: null });
  });

  it('un salto tomado elige el sumador de saltos y la ALU compara', () => {
    const state = singleCycleState(
      slot({
        text: 'bge t0, a0, 0x44',
        control: { ...NO_CONTROL, reads_rs1: true, reads_rs2: true, branch: true },
        rs1: { reg: 5, value: 7, bypass: null, bypass_lane: null },
        rs2: { reg: 10, value: 6, bypass: null, bypass_lane: null },
        next_pc: 0x80,
        wires: wires({
          immediate: 0x40,
          alu: { function: 'ge', a_source: 'register', b_source: 'register', a: 7, b: 6, out: 1 },
          target: 0x80,
          pc_source: 'target',
        }),
      }),
      REGISTERS,
    );
    expect(state.muxes.pc).toBe(1);
    expect(state.wires.target).toEqual({ active: true, value: 0x80 });
    expect(state.wires.immediate.active).toBe(false);
    expect(state.wires.immediate_to_target).toEqual({ active: true, value: 0x40 });
    expect(state.wires.pc_to_target.active).toBe(true);
    expect(state.wires.rs2.active).toBe(true);
    expect(state.alu).toEqual({ label: '≥', out: 1 });
    expect(state.signals).toEqual(['ALU ≥', 'salto condicional']);
  });

  it('jal y jalr escriben pc + 4 y eligen su siguiente pc', () => {
    const jal = singleCycleState(
      slot({
        text: 'jal ra, 0x80',
        control: { ...NO_CONTROL, jump: true, write_source: 'link' },
        next_pc: 0x80,
        register_write: { reg: 1, value: 0x44 },
        wires: wires({ immediate: 0x40, target: 0x80, pc_source: 'target' }),
      }),
      REGISTERS,
    );
    expect(jal.muxes).toEqual({ pc: 1, alu_a: null, alu_b: null, write_back: 2 });
    expect(jal.wires.link.active).toBe(true);
    expect(jal.alu).toBeNull();
    const jalr = singleCycleState(
      slot({
        text: 'jalr zero, 0(ra)',
        control: { ...NO_CONTROL, reads_rs1: true, alu_source: 'immediate', jump: true },
        rs1: { reg: 1, value: 0x44, bypass: null, bypass_lane: null },
        next_pc: 0x44,
        wires: wires({
          immediate: 0,
          alu: {
            function: 'add',
            a_source: 'register',
            b_source: 'immediate',
            a: 0x44,
            b: 0,
            out: 0x44,
          },
          pc_source: 'alu',
        }),
      }),
      REGISTERS,
    );
    expect(jalr.muxes.pc).toBe(2);
    expect(jalr.wires.alu_to_pc.active).toBe(true);
    expect(jalr.wires.write_back.active).toBe(false);
  });

  it('lui y auipc eligen el primer operando de la ALU', () => {
    const control: Control = { ...NO_CONTROL, alu_source: 'immediate', write_source: 'alu' };
    const lui = singleCycleState(
      slot({
        control,
        register_write: { reg: 10, value: 0x2000 },
        wires: wires({
          immediate: 0x2000,
          alu: {
            function: 'add',
            a_source: 'zero',
            b_source: 'immediate',
            a: 0,
            b: 0x2000,
            out: 0x2000,
          },
        }),
      }),
      REGISTERS,
    );
    expect(lui.muxes.alu_a).toBe(2);
    expect(lui.wires.zero.active).toBe(true);
    const auipc = singleCycleState(
      slot({
        control,
        register_write: { reg: 10, value: 0x1040 },
        wires: wires({
          immediate: 0x1000,
          alu: {
            function: 'add',
            a_source: 'pc',
            b_source: 'immediate',
            a: 0x40,
            b: 0x1000,
            out: 0x1040,
          },
        }),
      }),
      REGISTERS,
    );
    expect(auipc.muxes.alu_a).toBe(1);
    expect(auipc.wires.pc_to_alu.active).toBe(true);
  });

  it('una carga trae el dato de memoria al registro', () => {
    const state = singleCycleState(
      slot({
        control: {
          ...NO_CONTROL,
          reads_rs1: true,
          alu_source: 'immediate',
          mem_read: true,
          write_source: 'memory',
        },
        rs1: { reg: 2, value: 0x100, bypass: null, bypass_lane: null },
        data_access: { kind: 'load', address: 0x100, width: 'word', value: 6 },
        register_write: { reg: 6, value: 6 },
        wires: wires({
          immediate: 0,
          alu: {
            function: 'add',
            a_source: 'register',
            b_source: 'immediate',
            a: 0x100,
            b: 0,
            out: 0x100,
          },
        }),
      }),
      REGISTERS,
    );
    expect(state.muxes.write_back).toBe(1);
    expect(state.wires.memory_read.active).toBe(true);
    expect(state.wires.alu_to_write_back.active).toBe(false);
    expect(state.signals).toContain('lee memoria');
    expect(state.signals).toContain('rd ← memoria');
  });

  it('escribir en x0 no enciende la escritura', () => {
    const state = singleCycleState(
      { ...ADDI, control: { ...ADDI_CONTROL, write_source: null }, register_write: null },
      REGISTERS,
    );
    expect(state.wires.write_back.active).toBe(false);
    expect(state.registers.rd).toBeNull();
    expect(state.written).toBeNull();
  });

  it('sin instrucción, o sin decodificar, todo queda apagado', () => {
    const empty = singleCycleState(null, REGISTERS);
    expect(active(empty)).toEqual([]);
    expect(empty.decoded).toBe(false);
    const fault = singleCycleState(
      slot({ word: null, text: null, next_pc: null, trap: 'búsqueda fuera del código' }),
      REGISTERS,
    );
    expect(active(fault)).toEqual([]);
    expect(fault.trap).toBe('búsqueda fuera del código');
    expect(fault.pc).toBe(0x40);
  });
});

describe('singleCycleSlot', () => {
  const base = { stages: [] } as unknown as Snapshot;
  it('solo da hueco en el procesador de un ciclo', () => {
    expect(singleCycleSlot(base)).toBeNull();
    const single = {
      stages: [{ stage: 'single', name: 'CPU', lanes: [ADDI] }],
    } as unknown as Snapshot;
    expect(singleCycleSlot(single)).toBe(ADDI);
    const piped = { stages: [{ stage: 'if', name: 'IF', lanes: [ADDI] }] } as unknown as Snapshot;
    expect(singleCycleSlot(piped)).toBeNull();
  });
});

describe('textos', () => {
  it('reparte las señales en dos líneas sin pasarse', () => {
    expect(splitSignals(['ALU add', 'B ← inmediato', 'lee memoria', 'rd ← memoria'])).toEqual([
      'ALU add · B ← inmediato',
      'lee memoria · rd ← memoria',
    ]);
    expect(splitSignals([])).toEqual(['', '']);
  });

  it('describe el ciclo con palabras', () => {
    expect(describeCycle(singleCycleState(null, REGISTERS))).toBe(
      'Todavía no se ha ejecutado ninguna instrucción.',
    );
    expect(describeCycle(singleCycleState(ADDI, REGISTERS))).toBe(
      'Último ciclo: addi a0, a1, -1 · ALU add · B ← inmediato · rd ← ALU · a0 = 4.',
    );
  });
});
