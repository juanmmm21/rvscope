import type { Diagnostic } from '@codemirror/lint';
import type { Text } from '@codemirror/state';
import type { AsmError } from '../core';

// El ensamblador cuenta columnas en caracteres (puntos de código) y CodeMirror en unidades
// UTF-16: con una tilde o un emoji en un comentario anterior, contar igual descolocaría la marca.

/** Desplazamiento UTF-16 dentro de `lineText` del carácter `index` (desde 0), sin pasarse. */
export function charIndexToOffset(lineText: string, index: number): number {
  let offset = 0;
  let seen = 0;
  for (const char of lineText) {
    if (seen >= index) {
      break;
    }
    offset += char.length;
    seen += 1;
  }
  return offset;
}

export function toDiagnostics(doc: Text, errors: readonly AsmError[]): Diagnostic[] {
  return errors.map((error) => {
    const line = doc.line(Math.min(Math.max(error.line, 1), doc.lines));
    const start = error.column - 1;
    return {
      from: line.from + charIndexToOffset(line.text, start),
      to: line.from + charIndexToOffset(line.text, start + Math.max(error.length, 1)),
      severity: 'error',
      source: 'ensamblador',
      message: error.message,
    };
  });
}
