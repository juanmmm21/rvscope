import { describe, expect, it } from 'vitest';
import { VALUE_FORMATS, asciiChar, formatValue, hexByte, isValueFormat } from './format';

describe('formatValue', () => {
  it('enseña hexadecimal con ocho cifras', () => {
    expect(formatValue(0x2d0, 'hex')).toBe('0x000002d0');
    expect(formatValue(0xffffffff, 'hex')).toBe('0xffffffff');
    expect(formatValue(-1, 'hex')).toBe('0xffffffff');
  });

  it('distingue con signo y sin signo', () => {
    expect(formatValue(0xfffffffc, 'signed')).toBe('-4');
    expect(formatValue(0xfffffffc, 'unsigned')).toBe('4294967292');
    expect(formatValue(0x80000000, 'signed')).toBe('-2147483648');
    expect(formatValue(0x7fffffff, 'signed')).toBe('2147483647');
    expect(formatValue(-1, 'unsigned')).toBe('4294967295');
  });

  it('lee ASCII en orden de memoria y marca lo no imprimible', () => {
    // "hola" cargado con lw.
    expect(formatValue(0x616c6f68, 'ascii')).toBe('hola');
    expect(formatValue(0x41, 'ascii')).toBe('A···');
    expect(formatValue(0x7f0a2000, 'ascii')).toBe('· ··');
  });
});

describe('ayudantes', () => {
  it('dan bytes con dos cifras y caracteres imprimibles', () => {
    expect(hexByte(0x0a)).toBe('0a');
    expect(hexByte(0x1ff)).toBe('ff');
    expect(asciiChar(0x7e)).toBe('~');
    expect(asciiChar(0x80)).toBe('·');
  });

  it('reconocen solo los formatos que existen', () => {
    expect(VALUE_FORMATS.map((option) => option.id)).toEqual([
      'hex',
      'signed',
      'unsigned',
      'ascii',
    ]);
    expect(isValueFormat('signed')).toBe(true);
    expect(isValueFormat('octal')).toBe(false);
  });
});
