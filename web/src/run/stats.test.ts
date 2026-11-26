import { describe, expect, it } from 'vitest';
import type { StageSnapshot, Stats } from '../core';
import { plural, runCounters } from './stats';

const stats: Stats = {
  cycles: 130,
  retired: 85,
  stalls: 10,
  flushed: 30,
  dual_issues: 1,
  cpi: 1.5294,
};

function stages(names: readonly StageSnapshot['stage'][], lanes: number): StageSnapshot[] {
  return names.map((stage) => ({ stage, name: stage, lanes: Array<null>(lanes).fill(null) }));
}

describe('runCounters', () => {
  it('el de un ciclo solo cuenta ciclos, instrucciones y CPI', () => {
    expect(runCounters({ stats, stages: stages(['single'], 1) })).toEqual([
      'ciclo 130',
      '85 instrucciones',
      'CPI 1.53',
    ]);
  });

  it('los segmentados añaden burbujas y vaciadas', () => {
    const counters = runCounters({ stats, stages: stages(['if', 'id', 'ex', 'mem', 'wb'], 1) });
    expect(counters.slice(3)).toEqual(['10 burbujas', '30 vaciadas']);
  });

  it('con dos vías también cuenta los pares', () => {
    const counters = runCounters({ stats, stages: stages(['if', 'id', 'ex', 'mem', 'wb'], 2) });
    expect(counters.at(-1)).toBe('1 par');
  });

  it('sin instrucciones retiradas el CPI no existe', () => {
    const counters = runCounters({
      stats: { ...stats, cycles: 0, retired: 0, cpi: null },
      stages: stages(['single'], 1),
    });
    expect(counters).toEqual(['ciclo 0', '0 instrucciones', 'CPI —']);
  });
});

describe('plural', () => {
  it('distingue uno de varios', () => {
    expect(plural(1, 'burbuja', 'burbujas')).toBe('1 burbuja');
    expect(plural(0, 'burbuja', 'burbujas')).toBe('0 burbujas');
  });
});
