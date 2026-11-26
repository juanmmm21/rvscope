import { hexWord } from '../inspect/format';
import { ALU_OUTLINE, BOXES, MUXES, VIEW_BOX, WIRES, WIRE_LABELS } from './layout';
import { MUX_IDS, WIRE_IDS, type MuxId, type SingleCycleState, type WireId } from './single';
import {
  box,
  mux,
  points,
  selectInput,
  showWire,
  splitSignals,
  svg,
  text,
  wire,
  type MuxParts,
  type WireParts,
} from './svg';

// Dibujo SVG de la ruta de datos del procesador de un ciclo. Se construye una vez y en cada
// ciclo solo se cambian clases y textos, para que pintar a velocidad máxima no cueste nada.

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
    this.#idle = svg('g', { class: 'dp-wires' }, root);
    this.#active = svg('g', { class: 'dp-wires' }, root);

    const wires = {} as Record<WireId, WireParts>;
    for (const id of WIRE_IDS) {
      wires[id] = wire(this.#idle, id, WIRES[id], WIRE_LABELS[id]);
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
    const layers = { idle: this.#idle, active: this.#active };
    for (const id of WIRE_IDS) {
      const { active, value } = state.wires[id];
      showWire(this.#wires[id], active, value === null ? '' : hexWord(value), layers);
    }
    for (const id of MUX_IDS) {
      selectInput(this.#muxes[id], MUXES[id], state.muxes[id]);
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
