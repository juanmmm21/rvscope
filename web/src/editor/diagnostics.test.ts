import { Text } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { charIndexToOffset, toDiagnostics } from './diagnostics';

describe('assembler errors in the editor', () => {
  it('converts character columns to UTF-16 offsets', () => {
    expect(charIndexToOffset('addi a0', 5)).toBe(5);
    expect(charIndexToOffset('ñ😀 x', 2)).toBe(3);
    expect(charIndexToOffset('abc', 10)).toBe(3);
  });

  it('marks the exact range on the right line', () => {
    const doc = Text.of(['# año 😀', 'mal a0, 1', 'addi a0, a0, 9999']);
    const [first, second] = toDiagnostics(doc, [
      { line: 2, column: 1, length: 3, message: 'mnemónico desconocido «mal»' },
      { line: 3, column: 14, length: 4, message: 'no cabe' },
    ]);
    expect(first).toMatchObject({ severity: 'error', message: 'mnemónico desconocido «mal»' });
    expect(doc.sliceString(first?.from ?? 0, first?.to ?? 0)).toBe('mal');
    expect(doc.sliceString(second?.from ?? 0, second?.to ?? 0)).toBe('9999');
  });

  it('never points outside the document', () => {
    const doc = Text.of(['nop']);
    const [error] = toDiagnostics(doc, [{ line: 7, column: 40, length: 2, message: 'x' }]);
    expect(error?.from).toBe(3);
    expect(error?.to).toBe(3);
  });
});
