import { hexWord } from '../inspect/format';
import {
  ALU_OUTLINE,
  BOXES,
  MUXES,
  VIEW_BOX,
  WIRES,
  WIRE_LABELS,
  type Box,
  type MuxShape,
  type Point,
} from './layout';
import { MUX_IDS, WIRE_IDS, type MuxId, type SingleCycleState, type WireId } from './single';

// Dibujo SVG de la ruta de datos del procesador de un ciclo. Se construye una vez y en cada
// ciclo solo se cambian clases y textos, para que pintar a velocidad máxima no cueste nada.

const SVG = 'http://www.w3.org/2000/svg';

type SvgTag = keyof SVGElementTagNameMap;

function svg<K extends SvgTag>(
  tag: K,
  attributes: Readonly<Record<string, string | number>> = {},
  parent?: SVGElement,
): SVGElementTagNameMap[K] {
  const node = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attributes)) {
    node.setAttribute(name, String(value));
  }
  parent?.append(node);
  return node;
}

function points(path: readonly Point[]): string {
  return path.map(([x, y]) => `${String(x)},${String(y)}`).join(' ');
}

function text(
  parent: SVGElement,
  x: number,
  y: number,
  className: string,
  anchor: 'start' | 'middle' | 'end' = 'middle',
  content = '',
): SVGTextElement {
  const node = svg('text', { x, y, class: className, 'text-anchor': anchor }, parent);
  node.textContent = content;
  return node;
}

function box(parent: SVGElement, shape: Box, title: string): SVGGElement {
  const group = svg('g', { class: 'dp-unit' }, parent);
  svg('rect', { x: shape.x, y: shape.y, width: shape.width, height: shape.height, rx: 2 }, group);
  text(group, shape.x + shape.width / 2, shape.y + 13, 'dp-title', 'middle', title);
  return group;
}

// Caben unos 34 caracteres de Geist Mono por línea en la caja de control.
const SIGNAL_LINE = 34;

export function splitSignals(signals: readonly string[]): readonly [string, string] {
  const lines: [string[], string[]] = [[], []];
  let length = 0;
  for (const signal of signals) {
    const added = (length === 0 ? 0 : 3) + signal.length;
    if (lines[1].length === 0 && length + added <= SIGNAL_LINE) {
      lines[0].push(signal);
      length += added;
    } else {
      lines[1].push(signal);
    }
  }
  return [lines[0].join(' · '), lines[1].join(' · ')];
}

interface MuxParts {
  readonly group: SVGGElement;
  readonly selection: SVGPolylineElement;
}

function mux(parent: SVGElement, shape: MuxShape): MuxParts {
  const group = svg('g', { class: 'dp-unit dp-mux' }, parent);
  const { x, y, width, height } = shape;
  const inset = 8;
  svg(
    'polygon',
    {
      points: points([
        [x, y],
        [x + width, y + inset],
        [x + width, y + height - inset],
        [x, y + height],
      ]),
    },
    group,
  );
  svg('title', {}, group).textContent = shape.title;
  const selection = svg('polyline', { class: 'dp-selection', points: '' }, group);
  return { group, selection };
}

interface WireParts {
  readonly group: SVGGElement;
  readonly label: SVGTextElement | null;
}

export interface DatapathParts {
  readonly pc: SVGTextElement;
  readonly word: SVGTextElement;
  readonly instruction: SVGTextElement;
  readonly signals: readonly [SVGTextElement, SVGTextElement];
  readonly trap: SVGTextElement;
  readonly ports: Readonly<Record<'rs1' | 'rs2' | 'rd', SVGTextElement>>;
  readonly alu: SVGTextElement;
  readonly aluOut: SVGTextElement;
  readonly dataKind: SVGTextElement;
  readonly dataAddress: SVGTextElement;
  readonly dataValue: SVGTextElement;
}

export class SingleCycleDiagram {
  readonly element: SVGSVGElement;
  readonly #idle: SVGGElement;
  readonly #active: SVGGElement;
  readonly #wires: Readonly<Record<WireId, WireParts>>;
  readonly #muxes: Readonly<Record<MuxId, MuxParts>>;
  readonly #parts: DatapathParts;
  readonly #units: Readonly<Record<string, SVGGElement>>;

  constructor(caption: string) {
    const root = svg('svg', {
      viewBox: `0 0 ${String(VIEW_BOX.width)} ${String(VIEW_BOX.height)}`,
      class: 'dp',
      role: 'img',
      'aria-label': caption,
    });
    this.element = root;
    // Los cables en uso van en su propio grupo, encima, para que un tramo compartido con un
    // cable apagado se vea encendido.
    this.#idle = svg('g', { class: 'dp-wires' }, root);
    this.#active = svg('g', { class: 'dp-wires' }, root);

    const wires = {} as Record<WireId, WireParts>;
    for (const id of WIRE_IDS) {
      const group = svg('g', { class: 'dp-wire', 'data-wire': id }, this.#idle);
      for (const path of WIRES[id]) {
        svg('polyline', { points: points(path) }, group);
      }
      const spot = WIRE_LABELS[id];
      const label =
        spot === undefined ? null : text(group, spot.x, spot.y, 'dp-value', spot.anchor);
      wires[id] = { group, label };
    }
    this.#wires = wires;

    const units = svg('g', { class: 'dp-units' }, root);
    const pc = box(units, BOXES.pc, 'PC');
    const adder = box(units, BOXES.adder, '');
    text(adder, BOXES.adder.x + 28, BOXES.adder.y + 30, 'dp-symbol', 'middle', '+4');
    const instructions = box(units, BOXES.instructions, 'Mem. instrucciones');
    const control = box(units, BOXES.control, 'Control');
    const registers = box(units, BOXES.registers, 'Registros');
    const immediate = box(units, BOXES.immediate, 'Inmediato');
    const target = box(units, BOXES.target, '');
    text(target, BOXES.target.x + 32, BOXES.target.y + 34, 'dp-symbol', 'middle', 'pc + inm');
    const data = box(units, BOXES.data, 'Mem. datos');

    const alu = svg('g', { class: 'dp-unit' }, units);
    svg('polygon', { points: points(ALU_OUTLINE) }, alu);
    text(alu, 698, 232, 'dp-title', 'middle', 'ALU');
    text(units, 582, 244, 'dp-port', 'end', '0');

    const r = BOXES.registers;
    const port = (y: number, name: string): SVGTextElement =>
      text(registers, r.x + 6, y + 4, 'dp-port', 'start', name);
    port(290, 'dato');
    const c = BOXES.control;
    const d = BOXES.data;
    this.#parts = {
      pc: text(pc, BOXES.pc.x + 43, BOXES.pc.y + 36, 'dp-value'),
      word: text(instructions, BOXES.instructions.x + 58, BOXES.instructions.y + 60, 'dp-value'),
      instruction: text(control, c.x + 10, c.y + 40, 'dp-instruction', 'start'),
      signals: [
        text(control, c.x + 10, c.y + 60, 'dp-port', 'start'),
        text(control, c.x + 10, c.y + 74, 'dp-port', 'start'),
      ],
      trap: text(control, c.x + 10, c.y + 90, 'dp-trap', 'start'),
      ports: { rs1: port(190, 'rs1'), rs2: port(220, 'rs2'), rd: port(250, 'rd') },
      alu: text(alu, 698, 254, 'dp-symbol'),
      aluOut: text(units, 692, 332, 'dp-value'),
      dataKind: text(data, d.x + 10, d.y + 40, 'dp-port', 'start'),
      dataAddress: text(data, d.x + 10, d.y + 60, 'dp-value', 'start'),
      dataValue: text(data, d.x + 10, d.y + 80, 'dp-value', 'start'),
    };
    this.#units = { pc, instructions, control, registers, immediate, target, data, alu };

    const muxes = {} as Record<MuxId, MuxParts>;
    for (const id of MUX_IDS) {
      muxes[id] = mux(units, MUXES[id]);
    }
    this.#muxes = muxes;
  }

  update(state: SingleCycleState): void {
    for (const id of WIRE_IDS) {
      const { group, label } = this.#wires[id];
      const { active, value } = state.wires[id];
      const target = active ? this.#active : this.#idle;
      if (group.parentNode !== target) {
        target.append(group);
      }
      group.dataset.active = String(active);
      if (label !== null) {
        label.textContent = value === null ? '' : hexWord(value);
      }
    }
    for (const id of MUX_IDS) {
      const shape = MUXES[id];
      const selected = state.muxes[id];
      const input = selected === null ? undefined : shape.inputs[selected];
      this.#muxes[id].selection.setAttribute(
        'points',
        input === undefined
          ? ''
          : points([
              [shape.x, input],
              [shape.x + shape.width, shape.output],
            ]),
      );
    }

    const parts = this.#parts;
    parts.pc.textContent = state.pc === null ? '' : hexWord(state.pc);
    parts.word.textContent = state.word === null ? '' : hexWord(state.word);
    parts.instruction.textContent = state.text ?? '';
    const [first, second] = splitSignals(state.signals);
    parts.signals[0].textContent = first;
    parts.signals[1].textContent = second;
    parts.trap.textContent = state.trap ?? '';
    parts.ports.rs1.textContent = `rs1 ${state.registers.rs1 ?? ''}`;
    parts.ports.rs2.textContent = `rs2 ${state.registers.rs2 ?? ''}`;
    parts.ports.rd.textContent = `rd ${state.registers.rd ?? ''}`;
    parts.alu.textContent = state.alu?.label ?? '';
    parts.aluOut.textContent = state.alu === null ? '' : hexWord(state.alu.out);
    const memory = state.memory;
    parts.dataKind.textContent = memory === null ? '' : memory.kind === 'load' ? 'lee' : 'escribe';
    parts.dataAddress.textContent = memory === null ? '' : `@ ${hexWord(memory.address)}`;
    parts.dataValue.textContent = memory === null ? '' : hexWord(memory.value);
    parts.dataValue.dataset.written = String(memory?.kind === 'store');

    const used: Readonly<Record<string, boolean>> = {
      pc: state.pc !== null,
      instructions: state.word !== null,
      control: state.decoded,
      registers:
        state.registers.rs1 !== null || state.registers.rs2 !== null || state.written !== null,
      immediate: state.wires.immediate.active,
      target: state.wires.target.active,
      data: memory !== null,
      alu: state.alu !== null,
    };
    for (const [name, group] of Object.entries(this.#units)) {
      group.dataset.active = String(used[name] ?? false);
    }
  }
}
