import type { ListingLine, Snapshot } from '../core';

// Traducciones entre líneas del código y direcciones, sacadas del listado del ensamblador.

export interface SourceMap {
  // Primera dirección de cada línea que genera código (una pseudo de varias usa la primera).
  readonly addressOfLine: ReadonlyMap<number, number>;
  readonly lineOfAddress: ReadonlyMap<number, number>;
}

export function sourceMap(listing: readonly ListingLine[]): SourceMap {
  const addressOfLine = new Map<number, number>();
  const lineOfAddress = new Map<number, number>();
  for (const entry of listing) {
    if (!addressOfLine.has(entry.line)) {
      addressOfLine.set(entry.line, entry.address);
    }
    lineOfAddress.set(entry.address, entry.line);
  }
  return { addressOfLine, lineOfAddress };
}

// Las líneas sin código (comentarios, etiquetas solas, directivas) no paran nada.
export function breakpointAddresses(lines: Iterable<number>, map: SourceMap): number[] {
  const addresses = new Set<number>();
  for (const line of lines) {
    const address = map.addressOfLine.get(line);
    if (address !== undefined) {
      addresses.add(address);
    }
  }
  return [...addresses].sort((a, b) => a - b);
}

export type LocationKind = 'next' | 'halted' | 'fault';

export interface ExecutionLocation {
  readonly line: number;
  readonly kind: LocationKind;
}

// Línea que se marca en el editor. Coincide con donde paran los puntos de parada: en el de un
// ciclo, la instrucción que va a ejecutarse (`pc`); en los segmentados, la que acaba de entrar
// en IF. Parado, la línea donde terminó.
export function executionLocation(snapshot: Snapshot, map: SourceMap): ExecutionLocation | null {
  if (snapshot.halt !== null) {
    if (snapshot.halt.line === null) {
      return null;
    }
    return {
      line: snapshot.halt.line,
      // Un `ebreak` también termina el programa, pero no es un fallo.
      kind: snapshot.halt.kind === 'fault' ? 'fault' : 'halted',
    };
  }
  const fetch = snapshot.stages.find((stage) => stage.stage === 'if');
  if (fetch === undefined) {
    const line = map.lineOfAddress.get(snapshot.pc);
    return line === undefined ? null : { line, kind: 'next' };
  }
  for (const slot of fetch.lanes) {
    if (slot !== null && slot.line !== null) {
      return { line: slot.line, kind: 'next' };
    }
  }
  return null;
}
