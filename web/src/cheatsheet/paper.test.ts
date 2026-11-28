import { describe, expect, it } from 'vitest';
import type { CheatInstruction, CheatSheet, EncodedField, RegisterInfo } from '../core';
import { paperFormats, paperSheet, registerRanges } from './paper';

function instruction(
  name: string,
  format: CheatInstruction['format'],
  fields: EncodedField[],
): CheatInstruction {
  return {
    name,
    syntax: `${name} rd, rs1, imm`,
    summary: name,
    operation: `rd = ${name}`,
    format,
    extension: 'I',
    fields,
    example: name,
    example_word: null,
  };
}

const field = (name: string, high: number, low: number, value: number | null): EncodedField => ({
  name,
  high,
  low,
  value,
});

const SLLI = instruction('slli', 'I', [
  field('funct7', 31, 25, 0),
  field('shamt', 24, 20, null),
  field('rs1', 19, 15, null),
  field('funct3', 14, 12, 1),
  field('rd', 11, 7, null),
  field('opcode', 6, 0, 0b0010011),
]);

const ADDI = instruction('addi', 'I', [
  field('imm[11:0]', 31, 20, null),
  field('rs1', 19, 15, null),
  field('funct3', 14, 12, 0),
  field('rd', 11, 7, null),
  field('opcode', 6, 0, 0b0010011),
]);

const LUI = instruction('lui', 'U', [
  field('imm[31:12]', 31, 12, null),
  field('rd', 11, 7, null),
  field('opcode', 6, 0, 0b0110111),
]);

function register(
  index: number,
  abi: string,
  role: string,
  saver: RegisterInfo['saver'],
): RegisterInfo {
  return { index, name: `x${String(index)}`, abi, role, saver };
}

describe('paperFormats', () => {
  it('dibuja cada formato con su instrucción de libro, en orden R I S B U J', () => {
    const formats = paperFormats([LUI, SLLI, ADDI]);
    expect(formats.map((format) => format.format)).toEqual(['I', 'U']);
    expect(formats[0]?.fields.map((cell) => cell.name)).toEqual([
      'imm[11:0]',
      'rs1',
      'funct3',
      'rd',
      'opcode',
    ]);
    expect(formats[0]?.fields[0]).toEqual({ name: 'imm[11:0]', range: '31–20', width: 12 });
  });

  it('con un empate se queda con la primera instrucción de la tabla', () => {
    const xori = instruction('xori', 'I', [field('imm', 31, 20, null), ...ADDI.fields.slice(1)]);
    expect(paperFormats([ADDI, xori])[0]?.fields[0]?.name).toBe('imm[11:0]');
  });
});

describe('registerRanges', () => {
  it('junta los registros seguidos con el mismo uso y quién los guarda', () => {
    const ranges = registerRanges([
      register(0, 'zero', 'Constante cero', 'none'),
      register(5, 't0', 'Temporal / enlace', 'caller'),
      register(6, 't1', 'Temporal', 'caller'),
      register(7, 't2', 'Temporal', 'caller'),
      register(8, 's0', 'Guardado', 'callee'),
      register(18, 's2', 'Guardado', 'callee'),
      register(19, 's3', 'Guardado', 'callee'),
    ]);
    expect(ranges).toEqual([
      { names: 'x0 zero', detail: 'Constante cero' },
      { names: 'x5 t0', detail: 'Temporal / enlace · lo guarda quien llama' },
      { names: 'x6–x7 t1–t2', detail: 'Temporal · lo guarda quien llama' },
      // x8 y x18 tienen el mismo uso, pero no van seguidos.
      { names: 'x8 s0', detail: 'Guardado · lo guarda la función' },
      { names: 'x18–x19 s2–s3', detail: 'Guardado · lo guarda la función' },
    ]);
  });

  it('sin registros no hay tramos', () => {
    expect(registerRanges([])).toEqual([]);
  });
});

describe('paperSheet', () => {
  const sheet: CheatSheet = {
    groups: [{ id: 'arithmetic', title: 'Aritméticas', instructions: [ADDI] }],
    pseudo_instructions: [
      {
        name: 'li',
        syntax: 'li rd, imm',
        expansion: 'addi rd, x0, imm · o bien · lui rd, %hi(imm); addi rd, rd, %lo(imm)',
        summary: 'Carga',
        example: 'li a0, 1',
      },
    ],
    directives: [{ name: '.text', syntax: '.text', summary: 'Código', example: '.text' }],
    syscalls: [
      {
        number: 1,
        name: 'imprimir entero',
        kind: 'output',
        arguments: 'a0 = entero',
        result: '',
        description: 'Imprime.',
      },
    ],
    registers: [register(2, 'sp', 'Puntero de pila', 'callee')],
  };

  it('saca una línea por entrada en el mismo orden que el panel', () => {
    const paper = paperSheet(sheet).blocks;
    expect(paper.map((block) => [block.kind, block.id, block.title])).toEqual([
      ['instructions', 'paper-cs-arithmetic', 'Aritméticas'],
      ['pseudo', 'paper-cs-pseudo', 'Pseudoinstrucciones'],
      ['directives', 'paper-cs-directives', 'Directivas'],
      ['syscalls', 'paper-cs-syscalls', 'Llamadas al sistema (ecall)'],
      ['registers', 'paper-cs-registers', 'Registros y convenio ABI'],
    ]);
    expect(paper.map((block) => block.rows[0])).toEqual([
      { name: 'addi', syntax: 'addi rd, rs1, imm', detail: 'rd = addi', format: 'I' },
      {
        name: 'li',
        syntax: 'li rd, imm',
        detail: 'addi rd, x0, imm | lui rd, %hi(imm); addi rd, rd, %lo(imm)',
        format: null,
      },
      { name: '.text', syntax: '.text', detail: 'Código', format: null },
      { name: '', syntax: 'a7 = 1 · a0 = entero', detail: 'imprimir entero', format: null },
      {
        name: '',
        syntax: 'x2 sp',
        detail: 'Puntero de pila · lo guarda la función',
        format: null,
      },
    ]);
    expect(paperSheet(sheet).formats.map((format) => format.format)).toEqual(['I']);
  });
});
