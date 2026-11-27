import { describe, expect, it } from 'vitest';
import type { CheatSheet, PseudoInfo, SyscallInfo } from '../core';
import { expansionLines, saverLabel, sheetSections, syscallSignature } from './sheet';

const SHEET: CheatSheet = {
  groups: [
    {
      id: 'arithmetic',
      title: 'Aritméticas',
      instructions: [
        {
          name: 'add',
          syntax: 'add rd, rs1, rs2',
          summary: 'Suma',
          operation: 'rd = rs1 + rs2',
          format: 'R',
          extension: 'I',
        },
      ],
    },
    {
      id: 'multiply',
      title: 'Multiplicación (M)',
      instructions: [
        {
          name: 'mul',
          syntax: 'mul rd, rs1, rs2',
          summary: 'Multiplica',
          operation: 'rd = (rs1 * rs2)[31:0]',
          format: 'R',
          extension: 'M',
        },
      ],
    },
  ],
  pseudo_instructions: [
    { name: 'nop', syntax: 'nop', expansion: 'addi x0, x0, 0', summary: 'Nada', example: 'nop' },
  ],
  directives: [{ name: '.text', syntax: '.text', summary: 'Código', example: '.text' }],
  syscalls: [
    {
      number: 10,
      name: 'salir',
      kind: 'exit',
      arguments: '',
      result: '',
      description: 'Termina.',
    },
  ],
  registers: [{ index: 0, name: 'x0', abi: 'zero', role: 'Constante cero', saver: 'none' }],
};

describe('sheetSections', () => {
  it('pone los grupos del núcleo en su orden y detrás lo del ensamblador, ecalls y registros', () => {
    const sections = sheetSections(SHEET);
    expect(sections.map((section) => section.kind)).toEqual([
      'instructions',
      'instructions',
      'pseudo',
      'directives',
      'syscalls',
      'registers',
    ]);
    expect(sections.map((section) => section.title).slice(0, 2)).toEqual([
      'Aritméticas',
      'Multiplicación (M)',
    ]);
  });

  it('da a cada sección un id propio con prefijo', () => {
    const ids = sheetSections(SHEET).map((section) => section.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) {
      expect(id).toMatch(/^cs-[a-z]+$/u);
    }
  });

  it('no pierde ni inventa entradas', () => {
    const counts = sheetSections(SHEET).map((section) => section.entries.length);
    expect(counts).toEqual([1, 1, 1, 1, 1, 1]);
  });
});

describe('syscallSignature', () => {
  const base: SyscallInfo = {
    number: 5,
    name: 'leer entero',
    kind: 'input',
    arguments: '',
    result: 'a0 = entero',
    description: '',
  };

  it('dice el número de a7 y lo que devuelve', () => {
    expect(syscallSignature(base)).toBe('a7 = 5 · → a0 = entero');
  });

  it('pone los argumentos antes del resultado y omite lo vacío', () => {
    expect(syscallSignature({ ...base, number: 1, arguments: 'a0 = entero', result: '' })).toBe(
      'a7 = 1 · a0 = entero',
    );
    expect(syscallSignature({ ...base, number: 10, result: '' })).toBe('a7 = 10');
  });
});

describe('expansionLines', () => {
  const li: PseudoInfo = {
    name: 'li',
    syntax: 'li rd, imm',
    expansion: 'addi rd, x0, imm  ·  o bien  ·  lui rd, %hi(imm); addi rd, rd, %lo(imm)',
    summary: '',
    example: '',
  };

  it('separa las alternativas', () => {
    expect(expansionLines(li)).toEqual([
      'addi rd, x0, imm',
      'lui rd, %hi(imm); addi rd, rd, %lo(imm)',
    ]);
  });

  it('deja tal cual una expansión sin alternativas', () => {
    expect(expansionLines({ ...li, expansion: 'jalr x0, 0(ra)' })).toEqual(['jalr x0, 0(ra)']);
  });
});

describe('saverLabel', () => {
  it('nombra a quien conserva el registro', () => {
    expect(saverLabel('caller')).toBe('lo guarda quien llama');
    expect(saverLabel('callee')).toBe('lo guarda la función');
    expect(saverLabel('none')).toBe('—');
  });
});
