import type { Bypass, RegisterInfo, Slot, Snapshot } from '../core';
import {
  aluLabel,
  controlSignals,
  wireState,
  type MemoryAccessView,
  type WireState,
} from './single';

// Qué enseña el diagrama de los procesadores de 5 etapas en el último ciclo. Cada cable
// pertenece a una etapa y se enciende con lo que dice el núcleo de la instrucción que hay en
// ella; los multiplexores eligen según esas mismas señales. No se calcula nada del procesador.

export const PIPELINE_STAGES = ['if', 'id', 'ex', 'mem', 'wb'] as const;

export type PipelineStage = (typeof PIPELINE_STAGES)[number];

export const PIPELINE_WIRE_IDS = [
  // IF
  'next_pc',
  'if_pc',
  'if_pc_plus_4',
  'if_sequential',
  'if_instruction',
  // ID
  'id_pc',
  'id_pc_plus_4',
  'id_instruction',
  'id_rs1_index',
  'id_rs2_index',
  'id_control',
  'id_rs1',
  'id_rs2',
  'id_immediate',
  'id_rd',
  // EX
  'ex_pc_plus_4',
  'ex_rs1',
  'ex_rs2',
  'forward_mem_a',
  'forward_mem_b',
  'forward_wb_a',
  'forward_wb_b',
  'ex_operand_a',
  'ex_operand_b',
  'ex_store_data',
  'ex_pc_to_alu',
  'ex_pc_to_target',
  'ex_zero',
  'ex_immediate',
  'ex_immediate_to_target',
  'ex_alu_a',
  'ex_alu_b',
  'ex_alu_out',
  'ex_target',
  'ex_alu_to_pc',
  'ex_rd',
  // MEM
  'mem_pc_plus_4',
  'mem_address',
  'mem_store_data',
  'mem_alu_result',
  'mem_read',
  'mem_rd',
  // WB
  'wb_link',
  'wb_alu',
  'wb_memory',
  'wb_write_back',
  'wb_rd',
] as const;

export type PipelineWireId = (typeof PIPELINE_WIRE_IDS)[number];

export const PIPELINE_MUX_IDS = [
  'pc',
  'forward_a',
  'forward_b',
  'alu_a',
  'alu_b',
  'write_back',
] as const;

export type PipelineMuxId = (typeof PIPELINE_MUX_IDS)[number];

// Orden de las entradas de cada multiplexor, de arriba abajo: el dibujo las pone así.
export const PC_INPUTS = { sequential: 0, target: 1, alu: 2 } as const;
export const FORWARD_INPUTS = { register: 0, ExMem: 1, MemWb: 2 } as const;
export const ALU_A_INPUTS = { register: 0, pc: 1, zero: 2 } as const;
export const ALU_B_INPUTS = { register: 0, immediate: 1 } as const;
export const WRITE_BACK_INPUTS = { link: 0, alu: 1, memory: 2 } as const;

// `empty`: la etapa no tiene nada porque el procesador acaba de empezar o ya paró.
export type StageCondition = 'active' | 'stalled' | 'flushed' | 'bubble' | 'empty';

export interface StageCard {
  readonly stage: PipelineStage;
  readonly condition: StageCondition;
  readonly text: string | null;
  readonly trap: string | null;
}

export interface PipelineState {
  readonly stages: readonly StageCard[];
  readonly wires: Readonly<Record<PipelineWireId, WireState>>;
  readonly muxes: Readonly<Record<PipelineMuxId, number | null>>;
  // IF
  readonly pc: number | null;
  readonly word: number | null;
  // ID
  readonly decoding: string | null;
  readonly signals: readonly string[];
  // EX
  readonly alu: { readonly label: string; readonly out: number } | null;
  readonly bypasses: { readonly a: Bypass | null; readonly b: Bypass | null };
  // MEM
  readonly memory: MemoryAccessView | null;
  // Registros que se leen (ID) y el que se escribe (WB).
  readonly registers: {
    readonly rs1: string | null;
    readonly rs2: string | null;
    readonly rd: string | null;
  };
  readonly written: number | null;
}

type Lanes = Partial<Record<PipelineStage, { readonly slot: Slot | null }>>;

function lanesOf(snapshot: Snapshot | null): Lanes {
  const lanes: Lanes = {};
  if (snapshot === null || snapshot.cycle === 0) {
    return lanes;
  }
  for (const stage of snapshot.stages) {
    if (stage.stage !== 'single') {
      lanes[stage.stage] = { slot: stage.lanes[0] ?? null };
    }
  }
  return lanes;
}

function card(stage: PipelineStage, lane: { readonly slot: Slot | null } | undefined): StageCard {
  if (lane === undefined) {
    return { stage, condition: 'empty', text: null, trap: null };
  }
  const slot = lane.slot;
  if (slot === null) {
    return { stage, condition: 'bubble', text: null, trap: null };
  }
  return {
    stage,
    condition: slot.state,
    text: slot.text ?? (slot.trap === null ? null : 'instrucción sin decodificar'),
    trap: slot.trap,
  };
}

// Lo que sigue en el procesador: un hueco vaciado ya no cuenta para nada.
function live(lane: { readonly slot: Slot | null } | undefined): Slot | null {
  const slot = lane?.slot ?? null;
  return slot === null || slot.state === 'flushed' ? null : slot;
}

function registerName(registers: readonly RegisterInfo[], index: number): string {
  return registers[index]?.abi ?? `x${String(index)}`;
}

export function pipelineState(
  snapshot: Snapshot | null,
  registers: readonly RegisterInfo[],
): PipelineState {
  const lanes = lanesOf(snapshot);
  const fetch = live(lanes.if);
  const decode = live(lanes.id);
  const execute = live(lanes.ex);
  const memory = live(lanes.mem);
  const writeBack = live(lanes.wb);

  const idControl = decode?.control ?? null;
  const exControl = execute?.control ?? null;
  const exWires = execute?.wires ?? null;
  const alu = exWires?.alu ?? null;
  const memControl = memory?.control ?? null;
  const wbControl = writeBack?.control ?? null;
  const written = writeBack?.register_write ?? null;
  const wbSource = written === null ? null : (wbControl?.write_source ?? null);

  // El siguiente PC viene del salto que se resuelve en EX o, si no, de `pc + 4` de la búsqueda;
  // con IF retenida (o sin buscar) el PC no se escribe.
  const redirect =
    exWires?.pc_source === 'target' || exWires?.pc_source === 'alu' ? exWires.pc_source : null;
  const sequential = redirect === null && fetch?.state === 'active';
  const pcInput = redirect ?? (sequential ? 'sequential' : null);
  const nextPc =
    redirect !== null
      ? (execute?.next_pc ?? null)
      : sequential
        ? (fetch.wires?.pc_plus_4 ?? null)
        : null;

  const idRs1 = decode?.rs1 ?? null;
  const idRs2 = decode?.rs2 ?? null;
  const rs1 = execute?.rs1 ?? null;
  const rs2 = execute?.rs2 ?? null;
  const exLink = exControl?.write_source === 'link';
  const access = memory?.data_access ?? null;
  const memAccess =
    access !== null && access.kind !== 'fetch'
      ? { kind: access.kind, address: access.address, value: access.value }
      : null;

  const wires: Record<PipelineWireId, WireState> = {
    next_pc: wireState(pcInput !== null, nextPc),
    if_pc: wireState(fetch !== null),
    if_pc_plus_4: wireState(fetch !== null, fetch?.wires?.pc_plus_4 ?? null),
    if_sequential: wireState(sequential),
    if_instruction: wireState((fetch?.word ?? null) !== null),

    id_pc: wireState(decode !== null),
    id_pc_plus_4: wireState(idControl?.write_source === 'link'),
    id_instruction: wireState(decode !== null),
    id_rs1_index: wireState(idControl?.reads_rs1 ?? false),
    id_rs2_index: wireState(idControl?.reads_rs2 ?? false),
    id_control: wireState(idControl !== null),
    id_rs1: wireState(idRs1 !== null, idRs1?.value ?? null),
    id_rs2: wireState(idRs2 !== null, idRs2?.value ?? null),
    id_immediate: wireState(
      idControl !== null && (decode?.wires?.immediate ?? null) !== null,
      decode?.wires?.immediate ?? null,
    ),
    id_rd: wireState((idControl?.write_source ?? null) !== null),

    ex_pc_plus_4: wireState(exLink),
    ex_rs1: wireState(rs1 !== null && rs1.bypass === null, rs1?.value ?? null),
    ex_rs2: wireState(rs2 !== null && rs2.bypass === null, rs2?.value ?? null),
    forward_mem_a: wireState(rs1?.bypass === 'ExMem'),
    forward_mem_b: wireState(rs2?.bypass === 'ExMem'),
    forward_wb_a: wireState(rs1?.bypass === 'MemWb'),
    forward_wb_b: wireState(rs2?.bypass === 'MemWb'),
    ex_operand_a: wireState(alu?.a_source === 'register', rs1?.value ?? null),
    ex_operand_b: wireState(alu?.b_source === 'register', rs2?.value ?? null),
    ex_store_data: wireState(exControl?.mem_write ?? false, rs2?.value ?? null),
    ex_pc_to_alu: wireState(alu?.a_source === 'pc'),
    ex_pc_to_target: wireState((exWires?.target ?? null) !== null),
    ex_zero: wireState(alu?.a_source === 'zero'),
    ex_immediate: wireState(alu?.b_source === 'immediate', exWires?.immediate ?? null),
    ex_immediate_to_target: wireState(
      (exWires?.target ?? null) !== null,
      exWires?.immediate ?? null,
    ),
    ex_alu_a: wireState(alu !== null),
    ex_alu_b: wireState(alu !== null),
    // Lo que sale de la ALU solo sigue hacia MEM si es un resultado o una dirección: la
    // comparación de un salto y el destino de `jalr` se quedan en EX.
    ex_alu_out: wireState(
      alu !== null && exControl !== null && !exControl.branch && !exControl.jump,
      alu?.out ?? null,
    ),
    ex_target: wireState((exWires?.target ?? null) !== null, exWires?.target ?? null),
    ex_alu_to_pc: wireState(exWires?.pc_source === 'alu', execute?.next_pc ?? null),
    ex_rd: wireState((exControl?.write_source ?? null) !== null),

    mem_pc_plus_4: wireState(memControl?.write_source === 'link'),
    mem_address: wireState(memAccess !== null, memAccess?.address ?? null),
    mem_store_data: wireState(memAccess?.kind === 'store', memAccess?.value ?? null),
    mem_alu_result: wireState(memControl?.write_source === 'alu', memory?.result ?? null),
    mem_read: wireState(memAccess?.kind === 'load', memAccess?.value ?? null),
    mem_rd: wireState((memControl?.write_source ?? null) !== null),

    wb_link: wireState(wbSource === 'link'),
    wb_alu: wireState(wbSource === 'alu'),
    wb_memory: wireState(wbSource === 'memory'),
    wb_write_back: wireState(wbSource !== null, written?.value ?? null),
    wb_rd: wireState(wbSource !== null),
  };

  const forward = (operand: typeof rs1): number | null =>
    operand === null ? null : FORWARD_INPUTS[operand.bypass ?? 'register'];
  const muxes: Record<PipelineMuxId, number | null> = {
    pc: pcInput === null ? null : PC_INPUTS[pcInput],
    forward_a: forward(rs1),
    forward_b: forward(rs2),
    alu_a: alu === null ? null : ALU_A_INPUTS[alu.a_source],
    alu_b: alu === null ? null : ALU_B_INPUTS[alu.b_source],
    write_back: wbSource === null ? null : WRITE_BACK_INPUTS[wbSource],
  };

  return {
    stages: PIPELINE_STAGES.map((stage) => card(stage, lanes[stage])),
    wires,
    muxes,
    pc: lanes.if?.slot?.pc ?? null,
    word: lanes.if?.slot?.word ?? null,
    decoding: lanes.id?.slot?.text ?? null,
    signals:
      idControl === null ? [] : controlSignals(idControl, null, idControl.write_source ?? null),
    alu: alu === null ? null : { label: aluLabel(alu).toUpperCase(), out: alu.out },
    bypasses: { a: rs1?.bypass ?? null, b: rs2?.bypass ?? null },
    memory: memAccess,
    registers: {
      rs1: idRs1 === null ? null : registerName(registers, idRs1.reg),
      rs2: idRs2 === null ? null : registerName(registers, idRs2.reg),
      rd: written === null || wbSource === null ? null : registerName(registers, written.reg),
    },
    written: wbSource === null ? null : (written?.value ?? null),
  };
}

const STAGE_NAMES: Readonly<Record<PipelineStage, string>> = {
  if: 'IF',
  id: 'ID',
  ex: 'EX',
  mem: 'MEM',
  wb: 'WB',
};

const CONDITIONS: Readonly<Record<StageCondition, string>> = {
  active: '',
  stalled: 'retenida',
  flushed: 'vaciada',
  bubble: 'burbuja',
  empty: 'vacía',
};

export function stageName(stage: PipelineStage): string {
  return STAGE_NAMES[stage];
}

export function conditionLabel(condition: StageCondition): string {
  return CONDITIONS[condition];
}

const BYPASS_NAMES: Readonly<Record<Bypass, string>> = { ExMem: 'EX/MEM', MemWb: 'MEM/WB' };

export function bypassName(bypass: Bypass): string {
  return BYPASS_NAMES[bypass];
}

// Frase con lo mismo que el dibujo, para quien no lo ve.
export function describePipeline(state: PipelineState): string {
  if (state.stages.every((card) => card.condition === 'empty')) {
    return 'Todavía no se ha ejecutado ningún ciclo.';
  }
  const stages = state.stages
    .filter((card) => card.condition !== 'empty')
    .map((card) => {
      const parts = [stageName(card.stage)];
      if (card.condition === 'bubble') {
        parts.push('burbuja');
      } else {
        parts.push(card.text ?? '');
        if (card.condition !== 'active') {
          parts.push(`(${conditionLabel(card.condition)})`);
        }
      }
      return parts.join(' ');
    });
  const notes: string[] = [];
  if (state.bypasses.a !== null) {
    notes.push(`operando A adelantado desde ${bypassName(state.bypasses.a)}`);
  }
  if (state.bypasses.b !== null) {
    notes.push(`operando B adelantado desde ${bypassName(state.bypasses.b)}`);
  }
  if (state.written !== null && state.registers.rd !== null) {
    notes.push(`${state.registers.rd} = ${String(state.written)}`);
  }
  for (const card of state.stages) {
    if (card.trap !== null) {
      notes.push(card.trap);
    }
  }
  return `Último ciclo: ${[stages.join(' · '), ...notes].join('; ')}.`;
}
