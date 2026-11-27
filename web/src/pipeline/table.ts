import type { Bypass, PipelineTable, RegisterInfo, StageId, TraceBypass, TraceCell } from '../core';
import { hexWord } from '../inspect/format';
import { plural } from '../run/stats';

// Celda de la tabla ya preparada para pintar: lo que se ve y la frase que lo cuenta entero.
export interface TableCellView {
  readonly stage: string;
  // Vía en la doble emisión; `null` con una sola.
  readonly lane: number | null;
  readonly state: TraceCell['state'];
  readonly trap: boolean;
  // Marcas cortas de los operandos adelantados («←M», «←W1»…), en orden rs1, rs2.
  readonly bypasses: readonly string[];
  readonly current: boolean;
  readonly description: string;
}

export interface TableRowView {
  readonly label: string;
  readonly address: string;
  readonly line: number | null;
  // Una por columna; `null` si la instrucción no estaba en el pipeline en ese ciclo.
  readonly cells: readonly (TableCellView | null)[];
}

export interface TableView {
  readonly cycles: readonly number[];
  readonly current: number;
  readonly rows: readonly TableRowView[];
}

const STAGE_NAMES: Record<StageId, string> = {
  single: 'CPU',
  if: 'IF',
  id: 'ID',
  ex: 'EX',
  mem: 'MEM',
  wb: 'WB',
};

// Registro de segmentación del que sale el valor y la etapa en que está quien lo produjo.
const BYPASS_NAMES: Record<Bypass, { readonly latch: string; readonly mark: string }> = {
  ExMem: { latch: 'EX/MEM', mark: 'M' },
  MemWb: { latch: 'MEM/WB', mark: 'W' },
};

const OPERANDS = ['rs1', 'rs2'] as const;

export function stageName(stage: StageId): string {
  return STAGE_NAMES[stage];
}

function registerName(registers: readonly RegisterInfo[], index: number): string {
  return registers[index]?.abi ?? `x${String(index)}`;
}

// La vía solo se dice con dos: con una sería ruido.
export function bypassMark(bypass: TraceBypass, lanes: number): string {
  const lane = lanes > 1 ? String(bypass.lane) : '';
  return `←${BYPASS_NAMES[bypass.from].mark}${lane}`;
}

function describeCell(cell: TraceCell, registers: readonly RegisterInfo[], lanes: number): string {
  const lane = lanes > 1 ? `, vía ${String(cell.lane)}` : '';
  const parts = [`${stageName(cell.stage)} en el ciclo ${String(cell.cycle)}${lane}`];
  if (cell.state === 'stalled') {
    parts.push('retenida');
  } else if (cell.state === 'flushed') {
    parts.push('vaciada');
  }
  cell.bypasses.forEach((bypass, index) => {
    if (bypass === null) {
      return;
    }
    const lane = lanes > 1 ? ` de la vía ${String(bypass.lane)}` : '';
    parts.push(
      `${OPERANDS[index] ?? 'rs'} (${registerName(registers, bypass.reg)}) adelantado desde ${BYPASS_NAMES[bypass.from].latch}${lane}`,
    );
  });
  if (cell.trap) {
    parts.push('con excepción');
  }
  return parts.join(', ');
}

// Tabla instrucción × ciclo de la ventana que da el núcleo: una columna por ciclo, de
// `first_cycle` a `last_cycle`, y una fila por instrucción dinámica en orden de búsqueda.
export function tableView(
  table: PipelineTable,
  registers: readonly RegisterInfo[],
  lanes: number,
): TableView {
  const first = table.first_cycle;
  const last = table.last_cycle;
  const cycles =
    last === 0 ? [] : Array.from({ length: last - first + 1 }, (_, index) => first + index);
  const rows = table.rows.map((row): TableRowView => {
    const cells: (TableCellView | null)[] = cycles.map(() => null);
    for (const cell of row.cells) {
      const column = cell.cycle - first;
      if (column < 0 || column >= cells.length) {
        continue;
      }
      cells[column] = {
        stage: stageName(cell.stage),
        lane: lanes > 1 ? cell.lane : null,
        state: cell.state,
        trap: cell.trap,
        bypasses: cell.bypasses
          .filter((bypass): bypass is TraceBypass => bypass !== null)
          .map((bypass) => bypassMark(bypass, lanes)),
        current: cell.cycle === last,
        description: describeCell(cell, registers, lanes),
      };
    }
    return {
      label: row.text ?? 'sin decodificar',
      address: hexWord(row.pc),
      line: row.line,
      cells,
    };
  });
  return { cycles, current: last, rows };
}

// Resumen de la ventana para lectores de pantalla y para quien no quiera contar celdas.
export function describeTable(view: TableView): string {
  if (view.cycles.length === 0) {
    return 'Todavía no ha pasado ningún ciclo.';
  }
  const cells = view.rows.flatMap((row) => row.cells).filter((cell) => cell !== null);
  const count = (predicate: (cell: TableCellView) => boolean): number =>
    cells.filter(predicate).length;
  const first = view.cycles[0] ?? view.current;
  const range =
    first === view.current
      ? `Ciclo ${String(first)}`
      : `Ciclos ${String(first)} a ${String(view.current)}`;
  const stalled = count((cell) => cell.state === 'stalled');
  const flushed = count((cell) => cell.state === 'flushed');
  const forwarded = cells.reduce((total, cell) => total + cell.bypasses.length, 0);
  return [
    `${range}: ${plural(view.rows.length, 'instrucción', 'instrucciones')}`,
    plural(stalled, 'ciclo retenido', 'ciclos retenidos'),
    plural(flushed, 'vaciada', 'vaciadas'),
    plural(forwarded, 'operando adelantado', 'operandos adelantados'),
  ].join(' · ');
}
