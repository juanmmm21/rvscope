import type { RegisterInfo, Snapshot } from '../core';
import { SingleCycleDiagram } from './diagram';
import { singleCycleSlot, singleCycleState, type SingleCycleState } from './single';

export interface DatapathElements {
  readonly section: HTMLElement;
  readonly figure: HTMLElement;
  readonly caption: HTMLElement;
}

// Frase que dice con palabras lo mismo que el dibujo, para quien no lo ve.
export function describeCycle(state: SingleCycleState): string {
  if (state.text === null && state.trap === null) {
    return 'Todavía no se ha ejecutado ninguna instrucción.';
  }
  const parts = [state.text ?? 'instrucción sin decodificar', ...state.signals];
  if (state.written !== null && state.registers.rd !== null) {
    parts.push(`${state.registers.rd} = ${String(state.written)}`);
  }
  if (state.trap !== null) {
    parts.push(state.trap);
  }
  return `Último ciclo: ${parts.join(' · ')}.`;
}

// Panel de la ruta de datos. Solo se enseña con un procesador que tenga diagrama; el resto lo
// irán teniendo en los siguientes pasos de la Fase 5.
export class DatapathPanel {
  readonly #elements: DatapathElements;
  readonly #registers: readonly RegisterInfo[];
  readonly #single: SingleCycleDiagram;

  constructor(elements: DatapathElements, registers: readonly RegisterInfo[]) {
    this.#elements = elements;
    this.#registers = registers;
    this.#single = new SingleCycleDiagram('Ruta de datos del procesador de un ciclo');
    this.#single.element.setAttribute('aria-describedby', elements.caption.id);
    elements.figure.append(this.#single.element);
  }

  render(snapshot: Snapshot | null): void {
    const single = snapshot === null || snapshot.stages[0]?.stage === 'single';
    this.#elements.section.hidden = !single;
    if (!single) {
      return;
    }
    const state = singleCycleState(
      snapshot === null ? null : singleCycleSlot(snapshot),
      this.#registers,
    );
    this.#single.update(state);
    this.#elements.caption.textContent = describeCycle(state);
  }
}
