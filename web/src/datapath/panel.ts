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

export type DatapathKind = 'single' | 'pipeline';

// Qué diagrama le toca a un modelo: los segmentados, uno de 5 etapas por vía.
export function datapathKind(model: ModelInfo): DatapathKind {
  return model.pipelined ? 'pipeline' : 'single';
}

const LANE_ROLES = ['la más antigua', 'la más joven'];

// Panel de la ruta de datos: un diagrama por forma de procesador. Los de 5 etapas se crean al
// pedirlos y se rehacen si cambia el forwarding (añade o quita piezas) o el número de vías.
// Con dos vías se apilan: cada una tiene su ALU y sus cables, y el PC, el banco de registros
// (con puertos para las dos) y el único puerto de memoria se reparten entre ellas.
export class DatapathPanel {
  readonly #elements: DatapathElements;
  readonly #registers: readonly RegisterInfo[];
  #single: SingleCycleDiagram | null = null;
  #pipeline: { forwarding: boolean; diagrams: PipelineDiagram[] } | null = null;

  constructor(elements: DatapathElements, registers: readonly RegisterInfo[]) {
    this.#elements = elements;
    this.#registers = registers;
  }

  render(model: ModelInfo, snapshot: Snapshot | null): void {
    if (datapathKind(model) === 'single') {
      const diagram = this.#singleDiagram();
      this.#show([diagram]);
      const state = singleCycleState(
        snapshot === null ? null : singleCycleSlot(snapshot),
        this.#registers,
      );
      diagram.update(state);
      this.#elements.caption.textContent = describeCycle(state);
      return;
    }
    const diagrams = this.#pipelineDiagrams(model.forwarding, model.lanes);
    this.#show(diagrams);
    const states = diagrams.map((diagram, lane) => {
      const state = pipelineState(snapshot, this.#registers, lane);
      diagram.update(state);
      return state;
    });
    this.#elements.caption.textContent = describePipeline(...states);
  }

  #singleDiagram(): SingleCycleDiagram {
    this.#single ??= this.#describe(
      new SingleCycleDiagram('Ruta de datos del procesador de un ciclo'),
    );
    return this.#single;
  }

  #pipelineDiagrams(forwarding: boolean, lanes: number): PipelineDiagram[] {
    const current = this.#pipeline;
    if (current?.forwarding === forwarding && current.diagrams.length === lanes) {
      return current.diagrams;
    }
    const name = forwarding
      ? 'Ruta de datos del procesador de 5 etapas con forwarding'
      : 'Ruta de datos del procesador de 5 etapas';
    const diagrams =
      lanes === 1
        ? [this.#describe(new PipelineDiagram(name, forwarding))]
        : Array.from({ length: lanes }, (_, lane) => {
            const title = `Vía ${String(lane)}`;
            const subtitle = LANE_ROLES[lane] ?? '';
            return this.#describe(
              new PipelineDiagram(
                `Ruta de datos de la doble emisión, ${title.toLowerCase()}, ${subtitle}`,
                forwarding,
                { index: lane, title, subtitle },
              ),
            );
          });
    this.#pipeline = { forwarding, diagrams };
    return diagrams;
  }

  #describe<T extends { readonly element: SVGSVGElement }>(diagram: T): T {
    diagram.element.setAttribute('aria-describedby', this.#elements.caption.id);
    return diagram;
  }

  #show(diagrams: readonly { readonly element: SVGSVGElement }[]): void {
    const figure = this.#elements.figure;
    const children = figure.children;
    const same =
      children.length === diagrams.length &&
      diagrams.every((diagram, index) => children[index] === diagram.element);
    if (!same) {
      figure.replaceChildren(...diagrams.map((diagram) => diagram.element));
    }
  }
}
