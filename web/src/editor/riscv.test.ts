import { StringStream } from '@codemirror/language';
import { describe, expect, it } from 'vitest';
import type { Vocabulary } from '../core';
import { riscvParser, type RiscvState } from './riscv';

const VOCABULARY: Vocabulary = {
  instructions: ['addi', 'lw', 'sw', 'beq', 'ecall', 'lui'],
  pseudo_instructions: ['li', 'call', 'ret', 'j'],
  directives: ['.data', '.text', '.word', '.string'],
  registers: ['a0', 'x10', 'sp', 'x2', 'zero', 'x0', 'fp'],
  modifiers: ['hi', 'lo'],
};

// Devuelve los pares (texto, token) de cada línea sin los espacios.
function tokens(source: string): [string, string | null][] {
  const parser = riscvParser(VOCABULARY);
  const state: RiscvState = parser.startState?.(2) ?? { expectsOperation: true };
  const out: [string, string | null][] = [];
  for (const line of source.split('\n')) {
    const stream = new StringStream(line, 4, 2);
    while (!stream.eol()) {
      stream.start = stream.pos;
      const style = parser.token(stream, state);
      const text = stream.current();
      if (text.trim() !== '') {
        out.push([text, style]);
      }
    }
  }
  return out;
}

describe('riscv highlighting', () => {
  it('tells instructions, pseudo-instructions, registers and numbers apart', () => {
    expect(tokens('loop: addi a0, x10, -0x1F  # resta')).toEqual([
      ['loop', 'labelDef'],
      [':', 'punctuation'],
      ['addi', 'instruction'],
      ['a0', 'register'],
      [',', 'punctuation'],
      ['x10', 'register'],
      [',', 'punctuation'],
      ['-', 'operator'],
      ['0x1F', 'number'],
      ['# resta', 'comment'],
    ]);
    expect(tokens('LI a0, 5; call f')).toEqual([
      ['LI', 'pseudo'],
      ['a0', 'register'],
      [',', 'punctuation'],
      ['5', 'number'],
      [';', 'punctuation'],
      ['call', 'pseudo'],
      ['f', 'labelRef'],
    ]);
  });

  it('knows memory operands, modifiers and local labels', () => {
    expect(tokens('1: lw a0, %lo(n)(sp)\n   beq a0, zero, 1b')).toEqual([
      ['1', 'labelDef'],
      [':', 'punctuation'],
      ['lw', 'instruction'],
      ['a0', 'register'],
      [',', 'punctuation'],
      ['%lo', 'modifier'],
      ['(', 'punctuation'],
      ['n', 'labelRef'],
      [')', 'punctuation'],
      ['(', 'punctuation'],
      ['sp', 'register'],
      [')', 'punctuation'],
      ['beq', 'instruction'],
      ['a0', 'register'],
      [',', 'punctuation'],
      ['zero', 'register'],
      [',', 'punctuation'],
      ['1b', 'labelRef'],
    ]);
    expect(tokens('lui a0, %pcrel_hi(x)')[3]).toEqual(['%pcrel_hi', 'invalid']);
  });

  it('handles data, strings with escapes and unknown mnemonics', () => {
    expect(tokens('.data\nmsg: .string "di \\"hola\\" # no" # sí\nfoo a0')).toEqual([
      ['.data', 'directive'],
      ['msg', 'labelDef'],
      [':', 'punctuation'],
      ['.string', 'directive'],
      ['"di \\"hola\\" # no"', 'string'],
      ['# sí', 'comment'],
      ['foo', null],
      ['a0', 'register'],
    ]);
    expect(tokens(".word 'a', n << 2")).toEqual([
      ['.word', 'directive'],
      ["'a'", 'character'],
      [',', 'punctuation'],
      ['n', 'labelRef'],
      ['<', 'operator'],
      ['<', 'operator'],
      ['2', 'number'],
    ]);
  });
});
