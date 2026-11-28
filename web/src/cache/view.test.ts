import { describe, expect, it } from 'vitest';
import type { CacheOptions, CacheView } from '../core';
import {
  addressFields,
  counters,
  describeAccess,
  formatBytes,
  formatRate,
  hitChart,
  nearestPoint,
  powersOfTwo,
  setRows,
  sizeChoices,
} from './view';

const OPTIONS: CacheOptions = {
  max_sets: 1024,
  max_ways: 16,
  min_line_bytes: 4,
  max_line_bytes: 256,
  replacements: [],
  write_policies: [],
  default: {
    sets: 16,
    ways: 2,
    line_bytes: 16,
    replacement: 'lru',
    write_policy: 'write-back',
    write_allocate: true,
  },
};

// 2 conjuntos × 2 vías × 16 B: 27 bits de etiqueta, 1 de índice y 4 de desplazamiento.
const VIEW: CacheView = {
  settings: { ...OPTIONS.default, sets: 2 },
  size_bytes: 64,
  tag_bits: 27,
  index_bits: 1,
  offset_bits: 4,
  stats: {
    reads: 6,
    read_hits: 4,
    writes: 2,
    write_hits: 1,
    evictions: 1,
    writebacks: 1,
    memory_writes: 0,
    uncached: 0,
  },
  hit_rate: 5 / 8,
  history: [
    { accesses: 2, hits: 0 },
    { accesses: 4, hits: 2 },
    { accesses: 8, hits: 5 },
  ],
  sets: [
    [
      { valid: true, dirty: true, tag: 0x800000, address: 0x10000000, age: 1 },
      { valid: true, dirty: false, tag: 0x800001, address: 0x10000020, age: 0 },
    ],
    [
      { valid: false, dirty: false, tag: 0, address: null, age: null },
      { valid: false, dirty: false, tag: 0, address: null, age: null },
    ],
  ],
  recent: [
    {
      op: 'write',
      address: 0x10000024,
      tag: 0x800001,
      set: 0,
      way: 1,
      hit: false,
      eviction: { tag: 0x800002, address: 0x10000040, dirty: true },
    },
  ],
};

describe('opciones del formulario', () => {
  it('ofrece solo potencias de dos dentro de los límites', () => {
    expect(powersOfTwo(4, 256)).toEqual([4, 8, 16, 32, 64, 128, 256]);
    const choices = sizeChoices(OPTIONS);
    expect(choices.sets).toHaveLength(11);
    expect(choices.ways).toEqual([1, 2, 4, 8, 16]);
    expect(choices.lineBytes[0]).toBe(4);
  });

  it('escribe tamaños y tasas en castellano', () => {
    expect(formatBytes(512)).toBe('512 bytes');
    expect(formatBytes(4096)).toBe('4 KiB');
    expect(formatRate(0.625)).toBe('62,5 %');
    expect(formatRate(null)).toBe('—');
  });
});

describe('partes de la dirección', () => {
  it('parte la dirección en etiqueta, índice y desplazamiento', () => {
    const fields = addressFields(VIEW, 0x10000024);
    expect(fields.map((field) => [field.name, field.bits, field.high, field.low])).toEqual([
      ['etiqueta', 27, 31, 5],
      ['índice', 1, 4, 4],
      ['desplazamiento', 4, 3, 0],
    ]);
    expect(fields.map((field) => field.value)).toEqual(['0x0800001', '0x0', '0x4']);
  });

  it('con un solo conjunto el índice no tiene bits', () => {
    const fields = addressFields({ tag_bits: 28, index_bits: 0, offset_bits: 4 }, 0xfffffffc);
    expect(fields[0]?.value).toBe('0xfffffff');
    expect(fields[1]).toMatchObject({ bits: 0, high: null, value: null });
    expect(fields[2]?.value).toBe('0xc');
  });

  it('sin acceso enseña solo los bits', () => {
    expect(addressFields(VIEW, null).every((field) => field.value === null)).toBe(true);
  });
});

describe('conjuntos', () => {
  it('marca la vía del último acceso y describe cada línea', () => {
    const rows = setRows(VIEW);
    expect(rows.map((row) => row.recent)).toEqual([true, false]);
    const [dirty, loaded] = rows[0]?.lines ?? [];
    expect(dirty).toMatchObject({ tag: '0x800000', address: '0x10000000', recent: null });
    expect(dirty?.description).toBe(
      'Conjunto 0, vía 0: etiqueta 0x800000, bloque 0x10000000, sucia, 1 por delante',
    );
    expect(loaded?.recent).toBe('miss');
    expect(loaded?.age).toBe('sale la siguiente');
    expect(rows[1]?.lines[0]?.description).toBe('Conjunto 1, vía 0: vacía');
  });

  it('cuenta el acceso del último paso con su expulsión', () => {
    const [access] = VIEW.recent;
    expect(access === undefined ? '' : describeAccess(access)).toBe(
      'Escritura de 0x10000024: fallo · conjunto 0, vía 1 · expulsa el bloque 0x10000040 (sucio: se escribe en memoria)',
    );
  });

  it('una escritura sin asignación va directa a memoria', () => {
    expect(
      describeAccess({
        op: 'write',
        address: 8,
        tag: 0,
        set: 0,
        way: null,
        hit: false,
        eviction: null,
      }),
    ).toBe('Escritura de 0x00000008: fallo · sin asignar línea, directa a memoria');
  });
});

describe('contadores y gráfico', () => {
  it('resume aciertos, fallos y escrituras según la política', () => {
    const list = Object.fromEntries(
      counters(VIEW, 'data').map((counter) => [counter.label, counter.value]),
    );
    expect(list).toMatchObject({
      accesos: '8',
      aciertos: '5',
      fallos: '3',
      'tasa de acierto': '62,5 %',
      'aciertos en escrituras': '1 de 2',
      'bloques sucios escritos': '1',
    });
    expect(list['E/S sin caché']).toBeUndefined();
    const fetches = counters(VIEW, 'instruction').map((counter) => counter.label);
    expect(fetches).not.toContain('aciertos en escrituras');
  });

  it('dibuja la tasa acumulada de 0 a 100 % hasta el último acceso', () => {
    const chart = hitChart(VIEW.history);
    expect(chart.maxAccesses).toBe(8);
    expect(chart.points.map((point) => point.rate)).toEqual([0, 0.5, 0.625]);
    expect(chart.path.startsWith('M')).toBe(true);
    expect(chart.points[2]?.x).toBeGreaterThan(chart.points[1]?.x ?? Infinity);
    // Menos tasa, más abajo.
    expect(chart.points[0]?.y).toBeGreaterThan(chart.points[2]?.y ?? Infinity);
    expect(chart.description).toBe(
      'Tasa de acierto acumulada: 62,5 % tras 8 accesos (mínimo 0,0 %, máximo 62,5 %).',
    );
    expect(nearestPoint(chart, 10_000)?.accesses).toBe(8);
    expect(hitChart([]).description).toBe('Todavía no hay accesos.');
  });
});
