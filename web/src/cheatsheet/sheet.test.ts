import { describe, expect, it } from 'vitest';
import type { CheatInstruction, CheatSheet, EncodedField, PseudoInfo, SyscallInfo } from '../core';
import {
  directiveSearchText,
  expansionLines,
  fieldCells,
  foldText,
  hexWord,
  instructionSearchText,
  matchesTerms,
  pseudoSearchText,
  queryTerms,
  registerSearchText,
  saverLabel,
  searchSummary,
  sheetSections,
  syscallSearchText,
  syscallSignature,
} from './sheet';

const R_FIELDS = (funct7: number): EncodedField[] => [
  { name: 'funct7', high: 31, low: 25, value: funct7 },
  { name: 'rs2', high: 24, low: 20, value: null },
  { name: 'rs1', high: 19, low: 15, value: null },
  { name: 'funct3', high: 14, low: 12, value: 0 },
  { name: 'rd', high: 11, low: 7, value: null },
  { name: 'opcode', high: 6, low: 0, value: 0b0110011 },
];

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
          fields: R_FIELDS(0),
          example: 'add a0, a0, a1',
          example_word: 0x00b50533,
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
          fields: R_FIELDS(1),
          example: 'mul a0, a0, a1',
          example_word: 0x02b50533,
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

describe('fieldCells', () => {
  // addi sp, sp, -16: el inmediato negativo deja el bit 31 a uno.
  const addi: CheatInstruction = {
    name: 'addi',
    syntax: 'addi rd, rs1, imm',
    summary: 'Suma un inmediato',
    operation: 'rd = rs1 + imm',
    format: 'I',
    extension: 'I',
    fields: [
      { name: 'imm[11:0]', high: 31, low: 20, value: null },
      { name: 'rs1', high: 19, low: 15, value: null },
      { name: 'funct3', high: 14, low: 12, value: 0 },
      { name: 'rd', high: 11, low: 7, value: null },
      { name: 'opcode', high: 6, low: 0, value: 0b0010011 },
    ],
    example: 'addi sp, sp, -16',
    example_word: 0xff010113,
  };

  it('parte la palabra del ejemplo campo a campo', () => {
    expect(fieldCells(addi)).toEqual([
      { name: 'imm[11:0]', range: '31–20', width: 12, fixed: false, bits: '111111110000' },
      { name: 'rs1', range: '19–15', width: 5, fixed: false, bits: '00010' },
      { name: 'funct3', range: '14–12', width: 3, fixed: true, bits: '000' },
      { name: 'rd', range: '11–7', width: 5, fixed: false, bits: '00010' },
      { name: 'opcode', range: '6–0', width: 7, fixed: true, bits: '0010011' },
    ]);
  });

  it('sin palabra de ejemplo enseña solo los bits fijos', () => {
    const cells = fieldCells({ ...addi, example_word: null });
    expect(cells.map((cell) => cell.bits)).toEqual([null, null, '000', null, '0010011']);
  });

  it('un campo de un bit se nombra con un solo número', () => {
    const cells = fieldCells({
      ...addi,
      fields: [{ name: 'b', high: 31, low: 31, value: 1 }],
      example_word: null,
    });
    expect(cells[0]?.range).toBe('31');
  });
});

describe('hexWord', () => {
  it('escribe la palabra en hexadecimal con ocho cifras', () => {
    expect(hexWord(0xff010113)).toBe('0xff010113');
    expect(hexWord(0x73)).toBe('0x00000073');
  });
});

describe('buscador', () => {
  const [arithmetic, multiply] = SHEET.groups;
  const add = arithmetic?.instructions[0];
  const mul = multiply?.instructions[0];

  function finds(searchText: string, query: string): boolean {
    return matchesTerms(searchText, queryTerms(query));
  }

  it('pliega mayúsculas y tildes', () => {
    expect(foldText('División SIN Signo')).toBe('division sin signo');
    expect(queryTerms('  Multiplicación   rd ')).toEqual(['multiplicacion', 'rd']);
  });

  it('una búsqueda vacía lo deja todo', () => {
    expect(queryTerms('   ')).toEqual([]);
    expect(matchesTerms('lo que sea', [])).toBe(true);
  });

  it('encuentra una instrucción por mnemónico, qué hace, operación o formato', () => {
    if (add === undefined || mul === undefined) {
      throw new Error('faltan instrucciones de prueba');
    }
    const text = instructionSearchText(add);
    expect(finds(text, 'ADD')).toBe(true);
    expect(finds(text, 'suma')).toBe(true);
    expect(finds(text, 'rs1 + rs2')).toBe(true);
    expect(finds(text, 'formato r')).toBe(true);
    expect(finds(text, 'formato i')).toBe(false);
    expect(finds(instructionSearchText(mul), 'extensión M')).toBe(true);
    expect(finds(text, 'extension m')).toBe(false);
  });

  it('exige todas las palabras, en cualquier orden', () => {
    if (add === undefined) {
      throw new Error('falta add');
    }
    const text = instructionSearchText(add);
    expect(finds(text, 'rs2 suma')).toBe(true);
    expect(finds(text, 'suma resta')).toBe(false);
  });

  it('busca en pseudoinstrucciones por su expansión, y en directivas, ecalls y registros', () => {
    const [nop] = SHEET.pseudo_instructions;
    const [text] = SHEET.directives;
    const [exit] = SHEET.syscalls;
    const [zero] = SHEET.registers;
    if (nop === undefined || text === undefined || exit === undefined || zero === undefined) {
      throw new Error('faltan entradas de prueba');
    }
    expect(finds(pseudoSearchText(nop), 'addi')).toBe(true);
    expect(finds(directiveSearchText(text), 'código')).toBe(true);
    expect(finds(syscallSearchText(exit), 'a7 = 10')).toBe(true);
    expect(finds(syscallSearchText(exit), 'salir')).toBe(true);
    expect(finds(registerSearchText(zero), 'cero')).toBe(true);
    expect(finds(registerSearchText(zero), 'x0')).toBe(true);
  });

  it('resume el resultado', () => {
    expect(searchSummary('', 80)).toBe('');
    expect(searchSummary(' xyz ', 0)).toBe('Nada coincide con «xyz»');
    expect(searchSummary('add', 1)).toBe('1 coincidencia');
    expect(searchSummary('a', 7)).toBe('7 coincidencias');
  });
});
