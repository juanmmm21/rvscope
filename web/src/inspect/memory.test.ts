import { describe, expect, it } from 'vitest';
import type { DataAccess, Region, RegisterInfo, Slot, Snapshot, StageSnapshot } from '../core';
import {
  PAGE_BYTES,
  clampPage,
  lastDataAccesses,
  memoryRows,
  pageFor,
  parseAddress,
  regionAt,
  type AddressContext,
} from './memory';

function region(kind: Region['kind'], base: number, end: number): Region {
  return { kind, base, end, permissions: { read: true, write: kind !== 'text', execute: false } };
}

const regions = [
  region('text', 0, 0x40),
  region('data', 0x10000000, 0x20000000),
  region('stack', 0x70000000, 0x80000000),
  region('io', 0xffff0000, 2 ** 32),
];

function slot(access: DataAccess | null, state: Slot['state'] = 'active'): Slot {
  return {
    pc: 0,
    word: 0x13,
    text: 'nop',
    line: 1,
    state,
    control: null,
    rs1: null,
    rs2: null,
    result: null,
    next_pc: null,
    address: null,
    data_access: access,
    register_write: null,
    trap: null,
    wires: null,
  };
}

function snapshot(stages: StageSnapshot[]): Snapshot {
  return {
    model: 'single',
    status: 'ready',
    pc: 0,
    registers: [],
    changed: [],
    stats: { cycles: 0, retired: 0, stalls: 0, flushed: 0, dual_issues: 0, cpi: null },
    halt: null,
    output: '',
    pending_input: 0,
    input_closed: false,
    can_step_back: false,
    cycle: 0,
    stages,
  };
}

const store: DataAccess = { kind: 'store', address: 0x7fffffec, width: 'word', value: 6 };
const loadByte: DataAccess = { kind: 'load', address: 0x10000003, width: 'byte', value: 0 };

describe('lastDataAccesses', () => {
  it('toma el acceso de la única etapa del de un ciclo', () => {
    const view = snapshot([{ stage: 'single', name: 'Ciclo', lanes: [slot(store)] }]);
    expect(lastDataAccesses(view)).toEqual([{ kind: 'store', address: 0x7fffffec, size: 4 }]);
  });

  it('en los segmentados solo mira MEM, no WB, y no cuenta lo vaciado', () => {
    const view = snapshot([
      { stage: 'ex', name: 'EX', lanes: [slot(null)] },
      { stage: 'mem', name: 'MEM', lanes: [slot(loadByte), slot(store, 'flushed')] },
      { stage: 'wb', name: 'WB', lanes: [slot(store), null] },
    ]);
    expect(lastDataAccesses(view)).toEqual([{ kind: 'load', address: 0x10000003, size: 1 }]);
  });

  it('ignora la búsqueda de instrucciones y las burbujas', () => {
    const fetch: DataAccess = { kind: 'fetch', address: 0, width: 'word', value: 0x13 };
    const view = snapshot([{ stage: 'mem', name: 'MEM', lanes: [slot(fetch), null] }]);
    expect(lastDataAccesses(view)).toEqual([]);
  });
});

describe('páginas', () => {
  it('encuentra el segmento de una dirección', () => {
    expect(regionAt(0x10000004, regions)?.kind).toBe('data');
    expect(regionAt(0xfffffffc, regions)?.kind).toBe('io');
    expect(regionAt(0x40, regions)).toBeNull();
  });

  it('no se sale del segmento si cabe en él', () => {
    // La cima de la pila: la página acaba justo en el final del segmento.
    expect(pageFor(0x7ffffff0, regions[2] ?? null)).toBe(0x80000000 - PAGE_BYTES);
    // Más abajo, la fila de sp queda arriba.
    expect(pageFor(0x7ffffe28, regions[2] ?? null)).toBe(0x7ffffe20);
    // Un texto más corto que una página empieza en su base.
    expect(pageFor(0x30, regions[0] ?? null)).toBe(0);
    expect(pageFor(0x10000123, regions[1] ?? null)).toBe(0x10000120);
  });

  it('se queda dentro de los 32 bits', () => {
    expect(pageFor(0xfffffff8, null)).toBe(2 ** 32 - PAGE_BYTES);
    expect(clampPage(-16)).toBe(0);
    expect(clampPage(0x1234567)).toBe(0x1234560);
  });
});

describe('memoryRows', () => {
  const bytes = new Uint8Array(32);
  bytes.set([0x06, 0x00, 0x00, 0x00, 0x68, 0x6f, 0x6c, 0x61], 0);

  it('parte en palabras little-endian con su ASCII', () => {
    const rows = memoryRows(bytes, 0x10000000, 'word', { accesses: [], target: null, sp: null });
    expect(rows).toHaveLength(2);
    expect(rows[0]?.addressText).toBe('0x10000000');
    expect(rows[0]?.cells.map((cell) => cell.text)).toEqual([
      '0x00000006',
      '0x616c6f68',
      '0x00000000',
      '0x00000000',
    ]);
    expect(rows[0]?.ascii).toBe('····hola········');
    expect(rows[1]?.address).toBe(0x10000010);
  });

  it('en bytes da dieciséis celdas por fila', () => {
    const rows = memoryRows(bytes, 0, 'byte', { accesses: [], target: null, sp: null });
    expect(rows[0]?.cells).toHaveLength(16);
    expect(rows[0]?.cells[4]?.text).toBe('68');
  });

  it('marca accesos, destino y sp en la celda que los contiene', () => {
    const marks = {
      accesses: [
        { kind: 'load', address: 0x10000003, size: 1 },
        { kind: 'store', address: 0x10000014, size: 4 },
      ] as const,
      target: 0x10000009,
      sp: 0x1000001c,
    };
    const words = memoryRows(bytes, 0x10000000, 'word', marks).flatMap((row) => row.cells);
    expect(words.filter((cell) => cell.access !== null).map((cell) => cell.address)).toEqual([
      0x10000000, 0x10000014,
    ]);
    expect(words.find((cell) => cell.target)?.address).toBe(0x10000008);
    expect(words.find((cell) => cell.sp)?.address).toBe(0x1000001c);
    const single = memoryRows(bytes, 0x10000000, 'byte', marks).flatMap((row) => row.cells);
    expect(single.filter((cell) => cell.access === 'store')).toHaveLength(4);
    expect(single.filter((cell) => cell.access === 'load').map((cell) => cell.address)).toEqual([
      0x10000003,
    ]);
  });
});

describe('parseAddress', () => {
  const registers: RegisterInfo[] = [
    { index: 2, name: 'x2', abi: 'sp', role: 'pila', saver: 'callee' },
    { index: 8, name: 'x8', abi: 's0', role: 'marco', saver: 'callee' },
  ];
  const values: number[] = [];
  values[2] = 0x7ffffff0;
  values[8] = 0x10;
  const context: AddressContext = {
    labels: [{ name: 'n', address: 0x10000000 }],
    registers,
    values,
  };

  function address(text: string): number | string {
    const parsed = parseAddress(text, context);
    return parsed.ok ? parsed.address : parsed.message;
  }

  it('lee números en hex, binario y decimal', () => {
    expect(address('0x10000000')).toBe(0x10000000);
    expect(address('0xFFFF_0000')).toBe(0xffff0000);
    expect(address('0b1000')).toBe(8);
    expect(address(' 256 ')).toBe(256);
  });

  it('resuelve etiquetas y registros, con desplazamientos', () => {
    expect(address('n')).toBe(0x10000000);
    expect(address('n+4')).toBe(0x10000004);
    expect(address('sp - 8')).toBe(0x7fffffe8);
    expect(address('SP')).toBe(0x7ffffff0);
    expect(address('x2')).toBe(0x7ffffff0);
    expect(address('fp + n')).toBe(0x10000010);
  });

  it('da la vuelta en 32 bits', () => {
    expect(address('0 - 4')).toBe(0xfffffffc);
    expect(address('0xffffffff + 1')).toBe(0);
  });

  it('explica lo que no entiende', () => {
    expect(address('')).toBe('escribe una dirección, una etiqueta o un registro');
    expect(address('bucle')).toBe('no hay ninguna etiqueta ni registro «bucle»');
    expect(address('n 4')).toBe('falta un + o un - entre dos términos');
    expect(address('n +')).toBe('la expresión termina en un operador');
    expect(address('n + + 4')).toBe('sobra «+»');
    expect(address('n * 2')).toBe('no entiendo «* 2»');
    expect(address('0x100000000')).toBe('0x100000000 no cabe en 32 bits');
  });
});
