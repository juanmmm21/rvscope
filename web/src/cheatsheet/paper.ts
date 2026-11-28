import type { CheatInstruction, CheatSheet, InstructionFormat, RegisterInfo } from '../core';
import {
  expansionLines,
  fieldCells,
  saverLabel,
  sheetSections,
  syscallSignature,
  type SheetSection,
} from './sheet';

// La chuleta en papel: una hoja A4 que se lleva a clase. Lo mismo que el panel, pero cada entrada
// cabe en una línea (sintaxis y operación o expansión, sin fichas), los registros consecutivos
// con el mismo uso se juntan en un tramo y arriba van los seis formatos con sus campos de bits,
// que en el panel están repartidos por las fichas.

export interface PaperRow {
  readonly name: string;
  readonly syntax: string;
  readonly detail: string;
  // Formato de la instrucción; `null` en lo que no es una instrucción real.
  readonly format: InstructionFormat | null;
}

export interface PaperBlock {
  readonly kind: SheetSection['kind'];
  readonly id: string;
  readonly title: string;
  readonly rows: readonly PaperRow[];
}

export interface PaperField {
  readonly name: string;
  readonly range: string;
  readonly width: number;
}

export interface PaperFormat {
  readonly format: InstructionFormat;
  readonly fields: readonly PaperField[];
}

export interface Paper {
  readonly formats: readonly PaperFormat[];
  readonly blocks: readonly PaperBlock[];
}

const FORMATS: readonly InstructionFormat[] = ['R', 'I', 'S', 'B', 'U', 'J'];

export function paperSheet(sheet: CheatSheet): Paper {
  const instructions = sheet.groups.flatMap((group) => group.instructions);
  const blocks = sheetSections(sheet).map((section): PaperBlock => {
    const id = `paper-${section.id}`;
    switch (section.kind) {
      case 'instructions':
        return {
          kind: section.kind,
          id,
          title: section.title,
          rows: section.entries.map((instruction) => ({
            name: instruction.name,
            syntax: instruction.syntax,
            detail: instruction.operation,
            format: instruction.format,
          })),
        };
      case 'pseudo':
        return {
          kind: section.kind,
          id,
          title: section.title,
          rows: section.entries.map((pseudo) => ({
            name: pseudo.name,
            syntax: pseudo.syntax,
            detail: expansionLines(pseudo).join(' | '),
            format: null,
          })),
        };
      case 'directives':
        return {
          kind: section.kind,
          id,
          title: section.title,
          rows: section.entries.map((directive) => ({
            name: directive.name,
            syntax: directive.syntax,
            detail: directive.summary,
            format: null,
          })),
        };
      case 'syscalls':
        return {
          kind: section.kind,
          id,
          title: section.title,
          rows: section.entries.map((info) => ({
            name: '',
            syntax: syscallSignature(info),
            detail: info.name,
            format: null,
          })),
        };
      case 'registers':
        return {
          kind: section.kind,
          id,
          title: section.title,
          rows: registerRanges(section.entries).map((range) => ({
            name: '',
            syntax: range.names,
            detail: range.detail,
            format: null,
          })),
        };
    }
  });
  return { formats: paperFormats(instructions), blocks };
}

// El dibujo de cada formato sale de una instrucción real de la tabla: la que menos campos fijos
// tiene, porque es la que enseña el formato «de libro» (`addi` y no `slli` ni `ecall`, que fijan
// parte del inmediato). Un formato sin ninguna instrucción no se dibuja.
export function paperFormats(instructions: readonly CheatInstruction[]): PaperFormat[] {
  const formats: PaperFormat[] = [];
  for (const format of FORMATS) {
    let best: CheatInstruction | null = null;
    for (const instruction of instructions) {
      if (
        instruction.format === format &&
        (best === null || fixedCount(instruction) < fixedCount(best))
      ) {
        best = instruction;
      }
    }
    if (best !== null) {
      formats.push({
        format,
        fields: fieldCells(best).map(({ name, range, width }) => ({ name, range, width })),
      });
    }
  }
  return formats;
}

function fixedCount(instruction: CheatInstruction): number {
  return instruction.fields.filter((field) => field.value !== null).length;
}

export interface RegisterRange {
  // «x18–x27 s2–s11» o «x2 sp».
  readonly names: string;
  readonly detail: string;
}

// Registros seguidos con el mismo uso y quién los guarda van en una línea: en papel no hace falta
// repetir diez veces «Guardado».
export function registerRanges(registers: readonly RegisterInfo[]): RegisterRange[] {
  const runs: RegisterInfo[][] = [];
  for (const register of registers) {
    const run = runs.at(-1);
    const last = run?.at(-1);
    if (
      run !== undefined &&
      last !== undefined &&
      last.index + 1 === register.index &&
      last.role === register.role &&
      last.saver === register.saver
    ) {
      run.push(register);
    } else {
      runs.push([register]);
    }
  }
  return runs.flatMap((run) => {
    const first = run[0];
    const last = run.at(-1);
    if (first === undefined || last === undefined) {
      return [];
    }
    const names =
      first === last
        ? `${first.name} ${first.abi}`
        : `${first.name}–${last.name} ${first.abi}–${last.abi}`;
    const saver = saverLabel(first.saver);
    return [{ names, detail: first.saver === 'none' ? first.role : `${first.role} · ${saver}` }];
  });
}
