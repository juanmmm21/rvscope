import type { ListingLine } from '../core';

// Lo que se ve a la derecha de cada línea del editor: dirección, código máquina e instrucción
// real. Una pseudoinstrucción puede dar varias, que comparten fila.

export interface ListingRow {
  readonly address: string;
  readonly code: string;
  readonly text: string;
  // Detalle completo, una instrucción por línea, para el título de la fila.
  readonly detail: string;
}

export function hex32(value: number): string {
  return (value >>> 0).toString(16).padStart(8, '0');
}

export function listingRows(listing: readonly ListingLine[]): Map<number, ListingRow> {
  const byLine = new Map<number, ListingLine[]>();
  for (const entry of listing) {
    const group = byLine.get(entry.line);
    if (group === undefined) {
      byLine.set(entry.line, [entry]);
    } else {
      group.push(entry);
    }
  }
  const rows = new Map<number, ListingRow>();
  for (const [line, entries] of byLine) {
    const [first] = entries;
    if (first === undefined) {
      continue;
    }
    const pseudo = first.pseudo === null ? '' : ` (de ${first.pseudo})`;
    rows.set(line, {
      address: `0x${hex32(first.address)}`,
      code: entries.map((entry) => hex32(entry.word)).join(' '),
      text: entries.map((entry) => entry.text).join('; '),
      detail: entries
        .map((entry) => `0x${hex32(entry.address)}  ${hex32(entry.word)}  ${entry.text}`)
        .join('\n')
        .concat(pseudo),
    });
  }
  return rows;
}
