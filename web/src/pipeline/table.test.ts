import { describe, expect, it } from 'vitest';
import type { PipelineTable, RegisterInfo, TraceCell } from '../core';
import { bypassMark, describeTable, tableView } from './table';

const REGISTERS: RegisterInfo[] = ['zero', 'ra', 'sp', 'gp', 'tp', 't0', 't1', 't2']
  .concat(['s0', 's1', 'a0'])
  .map((abi, index) => ({ index, name: `x${String(index)}`, abi, role: '', saver: 'none' }));

function cell(cycle: number, stage: TraceCell['stage'], extra: Partial<TraceCell> = {}): TraceCell {
  return { cycle, stage, lane: 0, state: 'active', bypasses: [null, null], trap: false, ...extra };
}

// Carga y uso inmediato: el `add` espera un ciclo en ID y recibe a0 desde MEM/WB.
const TABLE: PipelineTable = {
  first_cycle: 3,
  last_cycle: 6,
  rows: [
    {
      pc: 0,
      text: 'lw a0, 0(sp)',
      line: 1,
      cells: [cell(3, 'ex'), cell(4, 'mem'), cell(5, 'wb')],
    },
    {
      pc: 4,
      text: 'add t0, a0, a0',
      line: 2,
      cells: [
        cell(3, 'id', { state: 'stalled' }),
        cell(4, 'id'),
        cell(5, 'ex', {
          bypasses: [
            { reg: 10, from: 'MemWb', lane: 0 },
            { reg: 10, from: 'MemWb', lane: 0 },
          ],
        }),
        cell(6, 'mem'),
      ],
    },
    {
      pc: 8,
      text: null,
      line: null,
      cells: [cell(5, 'if', { state: 'flushed', trap: true })],
    },
  ],
};

describe('tableView', () => {
  it('pone una columna por ciclo y cada celda en la suya', () => {
    const view = tableView(TABLE, REGISTERS, 1);
    expect(view.cycles).toEqual([3, 4, 5, 6]);
    expect(view.current).toBe(6);
    const stages = view.rows.map((row) => row.cells.map((entry) => entry?.stage ?? null));
    expect(stages).toEqual([
      ['EX', 'MEM', 'WB', null],
      ['ID', 'ID', 'EX', 'MEM'],
      [null, null, 'IF', null],
    ]);
    expect(view.rows[0]?.address).toBe('0x00000000');
    expect(view.rows[2]?.label).toBe('sin decodificar');
  });

  it('marca retenidas, vaciadas, excepciones, adelantamientos y el ciclo en curso', () => {
    const view = tableView(TABLE, REGISTERS, 1);
    const add = view.rows[1]?.cells ?? [];
    expect(add[0]?.state).toBe('stalled');
    expect(add[0]?.description).toBe('ID en el ciclo 3, retenida');
    expect(add[2]?.bypasses).toEqual(['←W', '←W']);
    expect(add[2]?.description).toBe(
      'EX en el ciclo 5, rs1 (a0) adelantado desde MEM/WB, rs2 (a0) adelantado desde MEM/WB',
    );
    expect(add[3]?.current).toBe(true);
    expect(add[2]?.current).toBe(false);
    expect(add[2]?.lane).toBeNull();
    const fetched = view.rows[2]?.cells[2];
    expect(fetched?.state).toBe('flushed');
    expect(fetched?.description).toBe('IF en el ciclo 5, vaciada, con excepción');
  });

  it('con dos vías dice la vía de cada celda y de dónde viene cada valor', () => {
    const table: PipelineTable = {
      first_cycle: 1,
      last_cycle: 1,
      rows: [
        {
          pc: 12,
          text: 'jalr zero, 0(ra)',
          line: 4,
          cells: [
            cell(1, 'ex', {
              lane: 0,
              bypasses: [{ reg: 1, from: 'ExMem', lane: 1 }, null],
            }),
          ],
        },
      ],
    };
    const only = tableView(table, REGISTERS, 2).rows[0]?.cells[0];
    expect(only?.lane).toBe(0);
    expect(only?.bypasses).toEqual(['←M1']);
    expect(only?.description).toBe(
      'EX en el ciclo 1, vía 0, rs1 (ra) adelantado desde EX/MEM de la vía 1',
    );
    expect(bypassMark({ reg: 1, from: 'ExMem', lane: 1 }, 1)).toBe('←M');
  });

  it('sin ciclos no hay columnas', () => {
    const view = tableView({ first_cycle: 0, last_cycle: 0, rows: [] }, REGISTERS, 1);
    expect(view.cycles).toEqual([]);
    expect(describeTable(view)).toBe('Todavía no ha pasado ningún ciclo.');
  });
});

describe('describeTable', () => {
  it('resume la ventana', () => {
    expect(describeTable(tableView(TABLE, REGISTERS, 1))).toBe(
      'Ciclos 3 a 6: 3 instrucciones · 1 ciclo retenido · 1 vaciada · 2 operandos adelantados',
    );
  });
});
