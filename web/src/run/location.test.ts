import { describe, expect, it } from 'vitest';
import type { ListingLine, Slot, Snapshot, StageSnapshot } from '../core';
import { breakpointAddresses, executionLocation, sourceMap } from './location';

function entry(address: number, line: number): ListingLine {
  return { address, word: 0x13, line, text: 'nop', pseudo: null };
}

// `li` de 32 bits en la línea 3: dos instrucciones.
const listing = [entry(0, 2), entry(4, 3), entry(8, 3), entry(12, 5)];
const map = sourceMap(listing);

function slot(pc: number, line: number | null): Slot {
  return {
    pc,
    word: 0x13,
    text: 'nop',
    line,
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
  };
}

function snapshot(pc: number, stages: StageSnapshot[], halt: Snapshot['halt'] = null): Snapshot {
  return {
    model: 'single',
    status: halt === null ? 'ready' : 'halted',
    pc,
    registers: [],
    changed: [],
    stats: { cycles: 0, retired: 0, stalls: 0, flushed: 0, dual_issues: 0, cpi: null },
    halt,
    output: '',
    pending_input: 0,
    input_closed: false,
    can_step_back: false,
    cycle: 0,
    stages,
  };
}

describe('sourceMap', () => {
  it('lleva cada línea a su primera dirección', () => {
    expect(map.addressOfLine.get(3)).toBe(4);
    expect(map.lineOfAddress.get(8)).toBe(3);
  });

  it('ignora líneas sin código y repetidas', () => {
    expect(breakpointAddresses([5, 1, 3, 3, 4], map)).toEqual([4, 12]);
  });
});

describe('executionLocation', () => {
  it('en el de un ciclo marca la instrucción de pc', () => {
    const single = [{ stage: 'single', name: 'Ciclo', lanes: [slot(4, 3)] } as const];
    expect(executionLocation(snapshot(12, single), map)).toEqual({ line: 5, kind: 'next' });
  });

  it('en los segmentados marca la primera vía de IF con línea', () => {
    const stages: StageSnapshot[] = [
      { stage: 'if', name: 'IF', lanes: [null, slot(8, 3)] },
      { stage: 'id', name: 'ID', lanes: [slot(0, 2), null] },
    ];
    expect(executionLocation(snapshot(12, stages), map)).toEqual({ line: 3, kind: 'next' });
  });

  it('parado marca donde terminó y si fue un fallo', () => {
    const fault = { pc: 4, line: 3, kind: 'fault', code: null, message: 'x' } as const;
    expect(executionLocation(snapshot(4, [], fault), map)).toEqual({ line: 3, kind: 'fault' });
    const ebreak = { pc: 12, line: 5, kind: 'breakpoint', code: null, message: 'x' } as const;
    expect(executionLocation(snapshot(12, [], ebreak), map)).toEqual({ line: 5, kind: 'halted' });
  });

  it('fuera del código no marca nada', () => {
    expect(executionLocation(snapshot(64, []), map)).toBeNull();
  });
});
