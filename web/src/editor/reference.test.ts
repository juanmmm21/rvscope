import { describe, expect, it } from 'vitest';
import type { CheatInstruction, CheatSheet } from '../core';
import { instructionFootnote, lookupReference, operationWordAt, referenceIndex } from './reference';

const LW: CheatInstruction = {
  name: 'lw',
  syntax: 'lw rd, offset(rs1)',
  summary: 'Carga una palabra',
  operation: 'rd = mem32[rs1 + offset]',
  format: 'I',
  extension: 'I',
  fields: [],
  example: 'lw a0, 0(sp)',
  example_word: null,
};

const SHEET: CheatSheet = {
  groups: [
    { id: 'load', title: 'Cargas', instructions: [LW] },
    {
      id: 'multiply',
      title: 'Multiplicación (M)',
      instructions: [{ ...LW, name: 'mul', extension: 'M', format: 'R' }],
    },
  ],
  pseudo_instructions: [
    {
      name: 'lw',
      syntax: 'lw rd, symbol',
      expansion: 'auipc…',
      summary: 'De una etiqueta',
      example: 'lw a0, label',
    },
    {
      name: 'li',
      syntax: 'li rd, imm',
      expansion: 'addi rd, x0, imm',
      summary: 'Constante',
      example: 'li a0, 1',
    },
  ],
  directives: [{ name: '.word', syntax: '.word valor', summary: 'Palabras', example: '.word 1' }],
  syscalls: [],
  registers: [],
};

function word(line: string, column: number): string | null {
  return operationWordAt(line, column)?.word ?? null;
}

describe('operationWordAt', () => {
  it('encuentra la operación con el cursor dentro, al principio o al final', () => {
    const line = '        addi sp, sp, -16';
    expect(operationWordAt(line, 10)).toEqual({ from: 8, to: 12, word: 'addi' });
    expect(word(line, 8)).toBe('addi');
    expect(word(line, 12)).toBe('addi');
  });

  it('no da ficha para operandos, espacios ni comentarios', () => {
    const line = '  add a0, a1, a2  # add otra vez';
    expect(word(line, 7)).toBeNull();
    expect(word(line, 1)).toBeNull();
    expect(word(line, 22)).toBeNull();
    // Un operando que se llama como una instrucción (una etiqueta «add») tampoco.
    expect(word('  j add', 6)).toBeNull();
  });

  it('salta las etiquetas de delante y sigue tras un punto y coma', () => {
    expect(word('loop: 1: beq a0, x0, done', 10)).toBe('beq');
    expect(word('n:  .word 6', 6)).toBe('.word');
    expect(word('  nop; li a0, 1', 8)).toBe('li');
  });

  it('una etiqueta que se llama como una instrucción no es una operación', () => {
    expect(word('add: nop', 1)).toBeNull();
  });

  it('una línea vacía o el cursor fuera de rango no rompen nada', () => {
    expect(word('', 0)).toBeNull();
    expect(word('nop', 99)).toBe('nop');
    expect(word('nop', -3)).toBe('nop');
  });
});

describe('referenceIndex', () => {
  const index = referenceIndex(SHEET);

  it('junta instrucción y pseudoinstrucción del mismo nombre, la instrucción primero', () => {
    expect(lookupReference(index, 'LW').map((entry) => entry.kind)).toEqual([
      'instruction',
      'pseudo',
    ]);
  });

  it('encuentra pseudoinstrucciones y directivas, y nada para lo desconocido', () => {
    expect(lookupReference(index, 'li').map((entry) => entry.kind)).toEqual(['pseudo']);
    expect(lookupReference(index, '.word').map((entry) => entry.kind)).toEqual(['directive']);
    expect(lookupReference(index, 'foo')).toEqual([]);
  });
});

describe('instructionFootnote', () => {
  it('dice formato y extensión', () => {
    expect(instructionFootnote(LW)).toBe('Formato I · base RV32I');
    expect(instructionFootnote({ ...LW, format: 'R', extension: 'M' })).toBe(
      'Formato R · extensión M',
    );
  });
});
