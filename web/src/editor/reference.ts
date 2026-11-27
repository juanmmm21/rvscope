import type { CheatInstruction, CheatSheet, DirectiveInfo, PseudoInfo } from '../core';

// Ficha del editor: qué hay en la chuleta para la palabra que está en la posición de operación
// de una sentencia. Los datos son los de `core.cheatSheet`; aquí solo se indexan y se localiza
// la palabra en la línea.

export type ReferenceEntry =
  | { readonly kind: 'instruction'; readonly info: CheatInstruction }
  | { readonly kind: 'pseudo'; readonly info: PseudoInfo }
  | { readonly kind: 'directive'; readonly info: DirectiveInfo };

// Varias entradas por nombre: `lw`, `jal` o `jalr` son instrucción y también pseudoinstrucción
// con otros operandos. La instrucción real va siempre primero.
export type ReferenceIndex = ReadonlyMap<string, readonly ReferenceEntry[]>;

export function referenceIndex(sheet: CheatSheet): ReferenceIndex {
  const index = new Map<string, ReferenceEntry[]>();
  const add = (name: string, entry: ReferenceEntry): void => {
    const key = name.toLowerCase();
    const list = index.get(key);
    if (list === undefined) {
      index.set(key, [entry]);
    } else {
      list.push(entry);
    }
  };
  for (const group of sheet.groups) {
    for (const info of group.instructions) {
      add(info.name, { kind: 'instruction', info });
    }
  }
  for (const info of sheet.pseudo_instructions) {
    add(info.name, { kind: 'pseudo', info });
  }
  for (const info of sheet.directives) {
    add(info.name, { kind: 'directive', info });
  }
  return index;
}

// Palabra de la línea en la columna dada (0 = antes del primer carácter), con sus límites.
export interface OperationWord {
  readonly from: number;
  readonly to: number;
  readonly word: string;
}

const WORD_CHAR = /[\w.$]/u;
// Lo que puede ir delante de la operación en su sentencia: espacios y etiquetas `nombre:` o `1:`.
const LEADING_LABELS = /^\s*(?:(?:[A-Za-z_.$][\w.$]*|\d+):\s*)*$/u;

// La palabra bajo `column` solo cuenta si es la operación de su sentencia: así un operando o una
// etiqueta que se llame como una instrucción no sacan ficha. Basta con mirar lo que hay delante
// en la línea (tras el último `;`), que no puede contener cadenas ni comentarios si es la
// operación; si delante hay un `#`, la palabra está en un comentario.
export function operationWordAt(line: string, column: number): OperationWord | null {
  let from = Math.min(Math.max(column, 0), line.length);
  let to = from;
  while (from > 0 && WORD_CHAR.test(line.charAt(from - 1))) {
    from -= 1;
  }
  while (to < line.length && WORD_CHAR.test(line.charAt(to))) {
    to += 1;
  }
  if (from === to) {
    return null;
  }
  const before = line.slice(0, from);
  if (before.includes('#')) {
    return null;
  }
  const statement = before.slice(before.lastIndexOf(';') + 1);
  if (!LEADING_LABELS.test(statement)) {
    return null;
  }
  // `addi:` es una etiqueta, no una operación.
  if (line.charAt(to) === ':') {
    return null;
  }
  return { from, to, word: line.slice(from, to) };
}

export function lookupReference(index: ReferenceIndex, word: string): readonly ReferenceEntry[] {
  return index.get(word.toLowerCase()) ?? [];
}

// Una línea con el formato y la extensión de una instrucción, para el pie de la ficha.
export function instructionFootnote(info: CheatInstruction): string {
  const extension = info.extension === 'M' ? 'extensión M' : 'base RV32I';
  return `Formato ${info.format} · ${extension}`;
}
