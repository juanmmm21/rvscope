import type { ModelInfo, RegisterInfo, Snapshot } from '../core';
import { SingleCycleDiagram } from './diagram';
import { describePipeline, pipelineState } from './pipeline';
import { PipelineDiagram } from './pipelineDiagram';
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

export type DatapathKind = 'single' | 'pipeline' | 'none';

// Qué diagrama le toca a un modelo; la doble emisión aún no tiene el suyo.
export function datapathKind(model: ModelInfo): DatapathKind {
  if (!model.pipelined) {
    return 'single';
  }
  return model.lanes === 1 ? 'pipeline' : 'none';
}

// Panel de la ruta de datos: un diagrama por forma de procesador. El de 5 etapas se crea al
// pedirlo y se rehace si cambia el forwarding, que añade o quita piezas.
export class DatapathPanel {
  readonly #elements: DatapathElements;
  readonly #registers: readonly RegisterInfo[];
  #single: SingleCycleDiagram | null = null;
  #pipeline: PipelineDiagram | null = null;

  constructor(elements: DatapathElements, registers: readonly RegisterInfo[]) {
    this.#elements = elements;
    this.#registers = registers;
  }

  render(model: ModelInfo, snapshot: Snapshot | null): void {
    const kind = datapathKind(model);
    this.#elements.section.hidden = kind === 'none';
    if (kind === 'single') {
      const diagram = this.#show(this.#singleDiagram());
      const state = singleCycleState(
        snapshot === null ? null : singleCycleSlot(snapshot),
        this.#registers,
      );
      diagram.update(state);
      this.#elements.caption.textContent = describeCycle(state);
    } else if (kind === 'pipeline') {
      const diagram = this.#show(this.#pipelineDiagram(model.forwarding));
      const state = pipelineState(snapshot, this.#registers);
      diagram.update(state);
      this.#elements.caption.textContent = describePipeline(state);
    }
  }

  #singleDiagram(): SingleCycleDiagram {
    this.#single ??= this.#describe(
      new SingleCycleDiagram('Ruta de datos del procesador de un ciclo'),
    );
    return this.#single;
  }

  #pipelineDiagram(forwarding: boolean): PipelineDiagram {
    if (this.#pipeline?.forwarding !== forwarding) {
      this.#pipeline = this.#describe(
        new PipelineDiagram(
          forwarding
            ? 'Ruta de datos del procesador de 5 etapas con forwarding'
            : 'Ruta de datos del procesador de 5 etapas',
          forwarding,
        ),
      );
    }
    return this.#pipeline;
  }

  #describe<T extends { readonly element: SVGSVGElement }>(diagram: T): T {
    diagram.element.setAttribute('aria-describedby', this.#elements.caption.id);
    return diagram;
  }

  #show<T extends { readonly element: SVGSVGElement }>(diagram: T): T {
    const figure = this.#elements.figure;
    if (figure.firstElementChild !== diagram.element) {
      figure.replaceChildren(diagram.element);
    }
    return diagram;
  }
}
