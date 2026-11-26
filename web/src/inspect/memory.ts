import type { Label, Region, RegisterInfo, Snapshot } from '../core';
import { asciiChar, hexByte, hexWord } from './format';

// Lógica de la vista de memoria sin DOM: qué página se ve, cómo se parte en celdas y qué se
// resalta. Los bytes los da siempre el núcleo (`readMemory`).

export type MemoryUnit = 'word' | 'byte';

export const ROW_BYTES = 16;
export const PAGE_ROWS = 16;
export const PAGE_BYTES = ROW_BYTES * PAGE_ROWS;

const ADDRESS_SPACE = 2 ** 32;
const LAST_PAGE = ADDRESS_SPACE - PAGE_BYTES;

export interface AccessMark {
  readonly kind: 'load' | 'store';
  readonly address: number;
  readonly size: number;
}

const WIDTH_BYTES = { byte: 1, half: 2, word: 4 } as const;

// Accesos a datos del último ciclo: los de la etapa MEM en los segmentados y el de la única
// etapa en el de un ciclo. Los de WB ya se vieron el ciclo anterior.
export function lastDataAccesses(snapshot: Snapshot): AccessMark[] {
  const stage = snapshot.stages.find((s) => s.stage === 'mem' || s.stage === 'single');
  if (stage === undefined) {
    return [];
  }
  const marks: AccessMark[] = [];
  for (const slot of stage.lanes) {
    if (slot === null || slot.state !== 'active' || slot.data_access === null) {
      continue;
    }
    const access = slot.data_access;
    if (access.kind === 'load' || access.kind === 'store') {
      marks.push({ kind: access.kind, address: access.address, size: WIDTH_BYTES[access.width] });
    }
  }
  return marks;
}

export function regionAt(address: number, regions: readonly Region[]): Region | null {
  return regions.find((region) => address >= region.base && address < region.end) ?? null;
}

function alignRow(address: number): number {
  return Math.floor(address / ROW_BYTES) * ROW_BYTES;
}

// Página que enseña `address`, sin salirse de su segmento si cabe en él: así el final de la
// pila o un segmento de datos corto no se ven rodeados de memoria que no existe.
export function pageFor(address: number, region: Region | null): number {
  let start = alignRow(address);
  if (region !== null) {
    const lastInRegion = alignRow(Math.max(region.base, region.end - PAGE_BYTES));
    start = Math.max(alignRow(region.base), Math.min(start, lastInRegion));
  }
  return clampPage(start);
}

export function clampPage(start: number): number {
  return Math.min(Math.max(alignRow(start), 0), LAST_PAGE);
}

export interface CellMarks {
  readonly accesses: readonly AccessMark[];
  // Dirección pedida con «Ir a», hasta la siguiente navegación.
  readonly target: number | null;
  readonly sp: number | null;
}

export interface MemoryCell {
  readonly address: number;
  readonly size: number;
  readonly text: string;
  readonly access: 'load' | 'store' | null;
  readonly target: boolean;
  readonly sp: boolean;
}

export interface MemoryRow {
  readonly address: number;
  readonly addressText: string;
  readonly cells: readonly MemoryCell[];
  readonly ascii: string;
}

function overlaps(start: number, size: number, other: number, otherSize: number): boolean {
  return start < other + otherSize && other < start + size;
}

export function memoryRows(
  bytes: Uint8Array,
  start: number,
  unit: MemoryUnit,
  marks: CellMarks,
): MemoryRow[] {
  const size = unit === 'word' ? 4 : 1;
  const rows: MemoryRow[] = [];
  for (let offset = 0; offset + ROW_BYTES <= bytes.length; offset += ROW_BYTES) {
    const rowAddress = start + offset;
    const cells: MemoryCell[] = [];
    let ascii = '';
    for (let i = 0; i < ROW_BYTES; i += size) {
      const address = rowAddress + i;
      let text: string;
      if (size === 4) {
        const value =
          (bytes[offset + i] ?? 0) |
          ((bytes[offset + i + 1] ?? 0) << 8) |
          ((bytes[offset + i + 2] ?? 0) << 16) |
          ((bytes[offset + i + 3] ?? 0) << 24);
        text = hexWord(value);
      } else {
        text = hexByte(bytes[offset + i] ?? 0);
      }
      const access = marks.accesses.find((mark) =>
        overlaps(address, size, mark.address, mark.size),
      );
      cells.push({
        address,
        size,
        text,
        access: access?.kind ?? null,
        target: marks.target !== null && overlaps(address, size, marks.target, 1),
        sp: marks.sp !== null && overlaps(address, size, marks.sp, 1),
      });
    }
    for (let i = 0; i < ROW_BYTES; i += 1) {
      ascii += asciiChar(bytes[offset + i] ?? 0);
    }
    rows.push({ address: rowAddress, addressText: hexWord(rowAddress), cells, ascii });
  }
  return rows;
}

export interface AddressContext {
  readonly labels: readonly Label[];
  readonly registers: readonly RegisterInfo[];
  readonly values: readonly number[];
}

export type ParsedAddress =
  | { readonly ok: true; readonly address: number }
  | { readonly ok: false; readonly message: string };

const TOKEN = /\s*(?:([+-])|(0x[0-9a-f_]+|0b[01_]+|[0-9][0-9_]*)|([A-Za-z_.$][\w.$]*))/iy;

function parseNumber(text: string): number {
  const digits = text.replaceAll('_', '');
  const lower = digits.toLowerCase();
  if (lower.startsWith('0x')) {
    return parseInt(lower.slice(2), 16);
  }
  if (lower.startsWith('0b')) {
    return parseInt(lower.slice(2), 2);
  }
  return parseInt(digits, 10);
}

function resolveName(name: string, context: AddressContext): number | null {
  const lower = name.toLowerCase();
  // `fp` es otro nombre de `s0`, como en el ensamblador.
  const register =
    lower === 'fp'
      ? context.registers.find((info) => info.index === 8)
      : context.registers.find((info) => info.abi === lower || info.name === lower);
  if (register !== undefined) {
    return context.values[register.index] ?? 0;
  }
  return context.labels.find((label) => label.name === name)?.address ?? null;
}

// «Ir a»: un número (hex, binario o decimal), una etiqueta o un registro, sumando o restando
// más términos: `0x10000000`, `n`, `sp`, `sp + 8`, `array + 0x10`.
export function parseAddress(input: string, context: AddressContext): ParsedAddress {
  const text = input.trim();
  if (text === '') {
    return { ok: false, message: 'escribe una dirección, una etiqueta o un registro' };
  }
  let total = 0;
  let sign: 1 | -1 | null = 1;
  let position = 0;
  TOKEN.lastIndex = 0;
  while (position < text.length) {
    TOKEN.lastIndex = position;
    const match = TOKEN.exec(text);
    if (match === null) {
      return { ok: false, message: `no entiendo «${text.slice(position).trim()}»` };
    }
    position = TOKEN.lastIndex;
    const [, operator, number, name] = match;
    if (operator !== undefined) {
      if (sign !== null) {
        return { ok: false, message: `sobra «${operator}»` };
      }
      sign = operator === '-' ? -1 : 1;
      continue;
    }
    if (sign === null) {
      return { ok: false, message: 'falta un + o un - entre dos términos' };
    }
    let value: number;
    if (number !== undefined) {
      value = parseNumber(number);
      if (!Number.isFinite(value) || value >= ADDRESS_SPACE) {
        return { ok: false, message: `${number} no cabe en 32 bits` };
      }
    } else if (name !== undefined) {
      const resolved = resolveName(name, context);
      if (resolved === null) {
        return { ok: false, message: `no hay ninguna etiqueta ni registro «${name}»` };
      }
      value = resolved >>> 0;
    } else {
      return { ok: false, message: `no entiendo «${text.slice(position).trim()}»` };
    }
    total += sign * value;
    sign = null;
  }
  if (sign !== null) {
    return { ok: false, message: 'la expresión termina en un operador' };
  }
  // Como en el procesador, las direcciones dan la vuelta en 32 bits.
  return { ok: true, address: ((total % ADDRESS_SPACE) + ADDRESS_SPACE) % ADDRESS_SPACE };
}
