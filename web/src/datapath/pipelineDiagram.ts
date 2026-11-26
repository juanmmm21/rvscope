import { hexWord } from '../inspect/format';
import {
  PIPELINE_MUX_IDS,
  PIPELINE_STAGES,
  PIPELINE_WIRE_IDS,
  conditionLabel,
  stageName,
  type PipelineMuxId,
  type PipelineStage,
  type PipelineState,
  type PipelineWireId,
} from './pipeline';
import {
  LATCHES,
  LATCH_BOTTOM,
  LATCH_TOP,
  LATCH_WIDTH,
  PIPELINE_ALU_OUTLINE,
  PIPELINE_BOXES,
  PIPELINE_VIEW_BOX,
  REGISTER_PORTS,
  STAGE_CENTERS,
  pipelineLayout,
} from './pipelineLayout';
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
import type { MuxShape } from './svg';

// Dibujo SVG de la ruta de datos de 5 etapas. Como el de un ciclo, se construye una vez y en
// cada ciclo solo cambian atributos y textos.

interface StageHeader {
  readonly group: SVGGElement;
  readonly name: SVGTextElement;
  readonly instruction: SVGTextElement;
}

interface PipelineParts {
  readonly pc: SVGTextElement;
  readonly word: SVGTextElement;
  readonly decoding: SVGTextElement;
  readonly signals: readonly [SVGTextElement, SVGTextElement];
  readonly ports: Readonly<Record<'rs1' | 'rs2' | 'rd', SVGTextElement>>;
  readonly alu: SVGTextElement;
  readonly dataKind: SVGTextElement;
  readonly dataAddress: SVGTextElement;
  readonly dataValue: SVGTextElement;
}

export class PipelineDiagram {
  readonly element: SVGSVGElement;
  readonly forwarding: boolean;
  readonly #layers: { readonly idle: SVGGElement; readonly active: SVGGElement };
  readonly #wires: Partial<Record<PipelineWireId, WireParts>>;
  readonly #muxes: Partial<Record<PipelineMuxId, { parts: MuxParts; shape: MuxShape }>>;
  readonly #headers: Readonly<Record<PipelineStage, StageHeader>>;
  readonly #parts: PipelineParts;
  readonly #units: Readonly<Record<string, SVGGElement>>;

  constructor(caption: string, forwarding: boolean) {
    this.forwarding = forwarding;
    const layout = pipelineLayout(forwarding);
    const root = svg('svg', {
      viewBox: `0 0 ${String(PIPELINE_VIEW_BOX.width)} ${String(PIPELINE_VIEW_BOX.height)}`,
      class: 'dp dp-pipeline',
      role: 'img',
      'aria-label': caption,
    });
    this.element = root;

    const headers = {} as Record<PipelineStage, StageHeader>;
    for (const stage of PIPELINE_STAGES) {
      const group = svg('g', { class: 'dp-stage', 'data-stage': stage }, root);
      const x = STAGE_CENTERS[stage];
      headers[stage] = {
        group,
        name: text(group, x, 14, 'dp-title', 'middle', stageName(stage)),
        instruction: text(group, x, 30, 'dp-stage-text'),
      };
    }
    this.#headers = headers;

    const latches = svg('g', { class: 'dp-latches' }, root);
    for (const latch of LATCHES) {
      svg(
        'rect',
        {
          class: 'dp-latch',
          x: latch.x,
          y: LATCH_TOP,
          width: LATCH_WIDTH,
          height: LATCH_BOTTOM - LATCH_TOP,
        },
        latches,
      );
      text(latches, latch.x + LATCH_WIDTH / 2, LATCH_TOP - 4, 'dp-title', 'middle', latch.name);
    }

    this.#layers = {
      idle: svg('g', { class: 'dp-wires' }, root),
      active: svg('g', { class: 'dp-wires' }, root),
    };
    const wires: Partial<Record<PipelineWireId, WireParts>> = {};
    for (const id of PIPELINE_WIRE_IDS) {
      const paths = layout.wires[id];
      if (paths !== undefined) {
        wires[id] = wire(this.#layers.idle, id, paths, layout.labels[id]);
      }
    }
    this.#wires = wires;

    const units = svg('g', { class: 'dp-units' }, root);
    const b = PIPELINE_BOXES;
    const pc = box(units, b.pc, 'PC');
    const adder = box(units, b.adder, '');
    text(adder, b.adder.x + b.adder.width / 2, b.adder.y + 29, 'dp-symbol', 'middle', '+4');
    const instructions = box(units, b.instructions, 'Mem. instrucciones');
    const control = box(units, b.control, 'Control');
    const registers = box(units, b.registers, 'Registros');
    const immediate = box(units, b.immediate, 'Inmediato');
    const target = box(units, b.target, '');
    text(target, b.target.x + 30, b.target.y + 30, 'dp-symbol', 'middle', 'pc + inm');
    const data = box(units, b.data, 'Mem. datos');
    const alu = svg('g', { class: 'dp-unit' }, units);
    svg('polygon', { points: points(PIPELINE_ALU_OUTLINE) }, alu);
    text(alu, 762, 284, 'dp-title', 'middle', 'ALU');
    text(units, 664, 284, 'dp-port', 'end', '0');

    const r = b.registers;
    const port = (y: number, name: string): SVGTextElement =>
      text(registers, r.x + 6, y + 4, 'dp-port', 'start', name);
    port(REGISTER_PORTS.data, 'dato');
    const c = b.control;
    const d = b.data;
    this.#parts = {
      pc: text(pc, b.pc.x + b.pc.width / 2, b.pc.y + 36, 'dp-value'),
      word: text(
        instructions,
        b.instructions.x + b.instructions.width / 2,
        b.instructions.y + 60,
        'dp-value',
      ),
      decoding: text(control, c.x + 10, c.y + 32, 'dp-instruction', 'start'),
      signals: [
        text(control, c.x + 10, c.y + 47, 'dp-port', 'start'),
        text(control, c.x + 10, c.y + 60, 'dp-port', 'start'),
      ],
      ports: {
        rs1: port(REGISTER_PORTS.rs1, 'rs1'),
        rs2: port(REGISTER_PORTS.rs2, 'rs2'),
        rd: port(REGISTER_PORTS.rd, 'rd'),
      },
      alu: text(alu, 762, 306, 'dp-symbol'),
      dataKind: text(data, d.x + 10, d.y + 34, 'dp-port', 'start'),
      dataAddress: text(data, d.x + 10, d.y + 82, 'dp-value', 'start'),
      dataValue: text(data, d.x + 10, d.y + 100, 'dp-value', 'start'),
    };
    this.#units = { pc, instructions, control, registers, immediate, target, data, alu };

    const muxes: Partial<Record<PipelineMuxId, { parts: MuxParts; shape: MuxShape }>> = {};
    for (const id of PIPELINE_MUX_IDS) {
      const shape = layout.muxes[id];
      if (shape !== undefined) {
        muxes[id] = { parts: mux(units, shape), shape };
      }
    }
    this.#muxes = muxes;
  }

  update(state: PipelineState): void {
    for (const id of PIPELINE_WIRE_IDS) {
      const parts = this.#wires[id];
      if (parts !== undefined) {
        const { active, value } = state.wires[id];
        showWire(parts, active, value === null ? '' : hexWord(value), this.#layers);
      }
    }
    for (const id of PIPELINE_MUX_IDS) {
      const entry = this.#muxes[id];
      if (entry !== undefined) {
        selectInput(entry.parts, entry.shape, state.muxes[id]);
      }
    }
    for (const card of state.stages) {
      const header = this.#headers[card.stage];
      const condition = conditionLabel(card.condition);
      header.group.dataset.condition = card.condition;
      header.name.textContent =
        condition === '' ? stageName(card.stage) : `${stageName(card.stage)} · ${condition}`;
      header.instruction.textContent = card.text ?? '';
    }

    const parts = this.#parts;
    parts.pc.textContent = state.pc === null ? '' : hexWord(state.pc);
    parts.word.textContent = state.word === null ? '' : hexWord(state.word);
    parts.decoding.textContent = state.decoding ?? '';
    const [first, second] = splitSignals(state.signals);
    parts.signals[0].textContent = first;
    parts.signals[1].textContent = second;
    parts.ports.rs1.textContent = `rs1 ${state.registers.rs1 ?? ''}`;
    parts.ports.rs2.textContent = `rs2 ${state.registers.rs2 ?? ''}`;
    parts.ports.rd.textContent = `rd ${state.registers.rd ?? ''}`;
    parts.alu.textContent = state.alu?.label ?? '';
    const memory = state.memory;
    parts.dataKind.textContent = memory === null ? '' : memory.kind === 'load' ? 'lee' : 'escribe';
    parts.dataAddress.textContent = memory === null ? '' : `@ ${hexWord(memory.address)}`;
    parts.dataValue.textContent = memory === null ? '' : hexWord(memory.value);
    parts.dataValue.dataset.written = String(memory?.kind === 'store');

    const wires = state.wires;
    const used: Readonly<Record<string, boolean>> = {
      pc: state.pc !== null,
      instructions: wires.if_instruction.active,
      control: wires.id_control.active,
      registers: wires.id_rs1.active || wires.id_rs2.active || wires.wb_write_back.active,
      immediate: wires.id_immediate.active,
      target: wires.ex_target.active,
      data: memory !== null,
      alu: state.alu !== null,
    };
    for (const [name, group] of Object.entries(this.#units)) {
      group.dataset.active = String(used[name] ?? false);
    }
  }
}
