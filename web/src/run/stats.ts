import type { Snapshot } from '../core';

// Contadores de la barra de estado. Cada modelo enseña los suyos: el de un ciclo no tiene
// burbujas ni vaciados, y los pares solo existen con dos vías. Se deciden por la forma de la
// vista que da el núcleo, no por el identificador, para no repetir aquí el catálogo de modelos.

export function plural(count: number, one: string, many: string): string {
  return `${String(count)} ${count === 1 ? one : many}`;
}

export function runCounters(snapshot: Pick<Snapshot, 'stats' | 'stages'>): string[] {
  const { stats, stages } = snapshot;
  const counters = [
    `ciclo ${String(stats.cycles)}`,
    plural(stats.retired, 'instrucción', 'instrucciones'),
    `CPI ${stats.cpi === null ? '—' : stats.cpi.toFixed(2)}`,
  ];
  if (stages.length > 1) {
    counters.push(
      plural(stats.stalls, 'burbuja', 'burbujas'),
      plural(stats.flushed, 'vaciada', 'vaciadas'),
    );
  }
  if (stages.some((stage) => stage.lanes.length > 1)) {
    counters.push(plural(stats.dual_issues, 'par', 'pares'));
  }
  return counters;
}
