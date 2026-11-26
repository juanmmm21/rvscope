import { describe, expect, it } from 'vitest';
import { listingRows } from './listing';

describe('listing beside the source', () => {
  it('groups the instructions of a pseudo-instruction in its line', () => {
    const rows = listingRows([
      { address: 0, word: 0x000185b7, line: 3, text: 'lui a1, 0x18', pseudo: 'li' },
      { address: 4, word: 0x6a058593, line: 3, text: 'addi a1, a1, 1696', pseudo: 'li' },
      { address: 8, word: 0x00000073, line: 5, text: 'ecall', pseudo: null },
    ]);
    expect([...rows.keys()]).toEqual([3, 5]);
    expect(rows.get(3)).toEqual({
      address: '0x00000000',
      code: '000185b7 6a058593',
      text: 'lui a1, 0x18; addi a1, a1, 1696',
      detail: '0x00000000  000185b7  lui a1, 0x18\n0x00000004  6a058593  addi a1, a1, 1696 (de li)',
    });
    expect(rows.get(5)?.code).toBe('00000073');
  });
});
