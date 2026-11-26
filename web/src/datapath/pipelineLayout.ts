import type { PipelineMuxId, PipelineStage, PipelineWireId } from './pipeline';
import type { Box, LabelSpot, MuxShape, Point } from './svg';

// Geometría del diagrama de 5 etapas, en unidades del `viewBox`. Cinco columnas separadas por
// los registros de segmentación; lo que vuelve hacia atrás (el salto al PC, la escritura en el
// banco de registros y los adelantamientos) rodea el diagrama por arriba y por abajo. Sin
// forwarding no se dibujan sus multiplexores: los operandos van directos de ID/EX a la ALU.

export const PIPELINE_VIEW_BOX = { width: 1240, height: 515 } as const;

export interface Latch {
  readonly name: string;
  readonly x: number;
}

export const LATCH_WIDTH = 12;
export const LATCH_TOP = 86;
export const LATCH_BOTTOM = 470;

export const LATCHES: readonly Latch[] = [
  { name: 'IF/ID', x: 280 },
  { name: 'ID/EX', x: 540 },
  { name: 'EX/MEM', x: 870 },
  { name: 'MEM/WB', x: 1080 },
];

// Centro de cada columna, para el nombre de la etapa y su instrucción.
export const STAGE_CENTERS: Readonly<Record<PipelineStage, number>> = {
  if: 140,
  id: 416,
  ex: 711,
  mem: 981,
  wb: 1166,
};

export const PIPELINE_BOXES = {
  pc: { x: 60, y: 235, width: 72, height: 50 },
  adder: { x: 160, y: 95, width: 50, height: 46 },
  instructions: { x: 160, y: 205, width: 104, height: 110 },
  control: { x: 318, y: 104, width: 205, height: 66 },
  registers: { x: 330, y: 188, width: 120, height: 162 },
  immediate: { x: 340, y: 386, width: 110, height: 36 },
  target: { x: 730, y: 104, width: 60, height: 48 },
  data: { x: 930, y: 230, width: 120, height: 130 },
} as const satisfies Readonly<Record<string, Box>>;

export const PIPELINE_ALU_OUTLINE: readonly Point[] = [
  [730, 225],
  [790, 255],
  [790, 330],
  [730, 360],
  [730, 307],
  [742, 292],
  [730, 277],
];

// Altura de cada puerto del banco de registros (lecturas a la izquierda, escritura abajo).
export const REGISTER_PORTS = { rs1: 215, rs2: 245, rd: 290, data: 335 } as const;

const COMMON_MUXES = {
  pc: {
    x: 22,
    y: 215,
    width: 20,
    height: 90,
    inputs: [230, 260, 290],
    output: 260,
    title: 'Siguiente PC',
  },
  alu_a: {
    x: 680,
    y: 215,
    width: 20,
    height: 80,
    inputs: [230, 255, 280],
    output: 255,
    title: 'Operando A',
  },
  alu_b: {
    x: 680,
    y: 305,
    width: 20,
    height: 50,
    inputs: [320, 340],
    output: 330,
    title: 'Operando B',
  },
  write_back: {
    x: 1140,
    y: 190,
    width: 20,
    height: 115,
    inputs: [205, 245, 292],
    output: 248,
    title: 'Dato para rd',
  },
} as const satisfies Readonly<Partial<Record<PipelineMuxId, MuxShape>>>;

const FORWARD_MUXES = {
  forward_a: {
    x: 574,
    y: 205,
    width: 18,
    height: 50,
    inputs: [215, 230, 245],
    output: 230,
    title: 'Adelantamiento de A',
  },
  forward_b: {
    x: 574,
    y: 295,
    width: 18,
    height: 50,
    inputs: [305, 320, 335],
    output: 320,
    title: 'Adelantamiento de B',
  },
} as const satisfies Readonly<Partial<Record<PipelineMuxId, MuxShape>>>;

type Paths = readonly (readonly Point[])[];

const COMMON_WIRES: Partial<Readonly<Record<PipelineWireId, Paths>>> = {
  next_pc: [
    [
      [42, 260],
      [60, 260],
    ],
  ],
  if_pc: [
    [
      [132, 260],
      [160, 260],
    ],
    [
      [146, 260],
      [146, 118],
      [160, 118],
    ],
    [
      [146, 182],
      [280, 182],
    ],
  ],
  if_pc_plus_4: [
    [
      [210, 118],
      [280, 118],
    ],
  ],
  if_sequential: [
    [
      [210, 118],
      [225, 118],
      [225, 72],
      [16, 72],
      [16, 230],
      [22, 230],
    ],
  ],
  if_instruction: [
    [
      [264, 260],
      [280, 260],
    ],
  ],
  id_pc: [
    [
      [292, 182],
      [540, 182],
    ],
  ],
  id_pc_plus_4: [
    [
      [292, 118],
      [300, 118],
      [300, 96],
      [540, 96],
    ],
  ],
  id_instruction: [
    [
      [292, 260],
      [304, 260],
      [304, 137],
      [318, 137],
    ],
    [
      [304, 260],
      [304, 404],
      [340, 404],
    ],
  ],
  id_rs1_index: [
    [
      [304, 215],
      [330, 215],
    ],
  ],
  id_rs2_index: [
    [
      [304, 245],
      [330, 245],
    ],
  ],
  id_control: [
    [
      [523, 137],
      [540, 137],
    ],
  ],
  id_rs1: [
    [
      [450, 215],
      [540, 215],
    ],
  ],
  id_rs2: [
    [
      [450, 305],
      [540, 305],
    ],
  ],
  id_immediate: [
    [
      [450, 404],
      [540, 404],
    ],
  ],
  id_rd: [
    [
      [304, 404],
      [304, 440],
      [540, 440],
    ],
  ],
  ex_pc_plus_4: [
    [
      [552, 96],
      [870, 96],
    ],
  ],
  ex_pc_to_alu: [
    [
      [552, 182],
      [670, 182],
      [670, 255],
      [680, 255],
    ],
  ],
  ex_pc_to_target: [
    [
      [552, 182],
      [706, 182],
      [706, 116],
      [730, 116],
    ],
  ],
  ex_zero: [
    [
      [668, 280],
      [680, 280],
    ],
  ],
  ex_immediate: [
    [
      [552, 404],
      [666, 404],
      [666, 340],
      [680, 340],
    ],
  ],
  ex_immediate_to_target: [
    [
      [552, 404],
      [712, 404],
      [712, 140],
      [730, 140],
    ],
  ],
  ex_alu_a: [
    [
      [700, 255],
      [730, 255],
    ],
  ],
  ex_alu_b: [
    [
      [700, 330],
      [730, 330],
    ],
  ],
  ex_alu_out: [
    [
      [790, 292],
      [870, 292],
    ],
  ],
  ex_target: [
    [
      [790, 128],
      [820, 128],
      [820, 60],
      [10, 60],
      [10, 260],
      [22, 260],
    ],
  ],
  ex_alu_to_pc: [
    [
      [790, 292],
      [850, 292],
      [850, 44],
      [4, 44],
      [4, 290],
      [22, 290],
    ],
  ],
  ex_rd: [
    [
      [552, 440],
      [870, 440],
    ],
  ],
  mem_pc_plus_4: [
    [
      [882, 96],
      [1080, 96],
    ],
  ],
  mem_address: [
    [
      [882, 292],
      [930, 292],
    ],
  ],
  mem_store_data: [
    [
      [882, 375],
      [910, 375],
      [910, 340],
      [930, 340],
    ],
  ],
  mem_alu_result: [
    [
      [882, 292],
      [906, 292],
      [906, 215],
      [1080, 215],
    ],
  ],
  mem_read: [
    [
      [1050, 292],
      [1080, 292],
    ],
  ],
  mem_rd: [
    [
      [882, 440],
      [1080, 440],
    ],
  ],
  wb_link: [
    [
      [1092, 96],
      [1120, 96],
      [1120, 205],
      [1140, 205],
    ],
  ],
  wb_alu: [
    [
      [1092, 215],
      [1112, 215],
      [1112, 245],
      [1140, 245],
    ],
  ],
  wb_memory: [
    [
      [1092, 292],
      [1140, 292],
    ],
  ],
  wb_write_back: [
    [
      [1160, 248],
      [1232, 248],
      [1232, 500],
      [314, 500],
      [314, REGISTER_PORTS.data],
      [330, REGISTER_PORTS.data],
    ],
  ],
  wb_rd: [
    [
      [1092, 440],
      [1100, 440],
      [1100, 480],
      [308, 480],
      [308, REGISTER_PORTS.rd],
      [330, REGISTER_PORTS.rd],
    ],
  ],
};

// Con forwarding, cada operando pasa por su multiplexor de adelantamiento; las entradas
// adelantadas suben desde el resultado de MEM (EX/MEM) y desde el dato que escribe WB (MEM/WB).
const FORWARD_WIRES: Partial<Readonly<Record<PipelineWireId, Paths>>> = {
  ex_rs1: [
    [
      [552, 215],
      [574, 215],
    ],
  ],
  ex_rs2: [
    [
      [552, 305],
      [574, 305],
    ],
  ],
  ex_operand_a: [
    [
      [592, 230],
      [680, 230],
    ],
  ],
  ex_operand_b: [
    [
      [592, 320],
      [680, 320],
    ],
  ],
  ex_store_data: [
    [
      [592, 320],
      [610, 320],
      [610, 375],
      [870, 375],
    ],
  ],
  forward_mem_a: [
    [
      [892, 292],
      [892, 460],
      [562, 460],
      [562, 230],
      [574, 230],
    ],
  ],
  forward_mem_b: [
    [
      [892, 292],
      [892, 460],
      [562, 460],
      [562, 320],
      [574, 320],
    ],
  ],
  forward_wb_a: [
    [
      [568, 500],
      [568, 245],
      [574, 245],
    ],
  ],
  forward_wb_b: [
    [
      [568, 500],
      [568, 335],
      [574, 335],
    ],
  ],
};

const DIRECT_WIRES: Partial<Readonly<Record<PipelineWireId, Paths>>> = {
  ex_rs1: [
    [
      [552, 215],
      [600, 215],
    ],
  ],
  ex_rs2: [
    [
      [552, 305],
      [600, 305],
    ],
  ],
  ex_operand_a: [
    [
      [600, 215],
      [660, 215],
      [660, 230],
      [680, 230],
    ],
  ],
  ex_operand_b: [
    [
      [600, 305],
      [660, 305],
      [660, 320],
      [680, 320],
    ],
  ],
  ex_store_data: [
    [
      [600, 305],
      [610, 305],
      [610, 375],
      [870, 375],
    ],
  ],
};

const COMMON_LABELS: Partial<Readonly<Record<PipelineWireId, LabelSpot>>> = {
  next_pc: { x: 22, y: 320, anchor: 'start' },
  if_pc_plus_4: { x: 245, y: 112, anchor: 'middle' },
  id_rs1: { x: 458, y: 209, anchor: 'start' },
  id_rs2: { x: 458, y: 299, anchor: 'start' },
  id_immediate: { x: 458, y: 398, anchor: 'start' },
  ex_immediate: { x: 556, y: 398, anchor: 'start' },
  ex_immediate_to_target: { x: 556, y: 398, anchor: 'start' },
  ex_store_data: { x: 616, y: 370, anchor: 'start' },
  ex_alu_out: { x: 796, y: 306, anchor: 'start' },
  ex_target: { x: 560, y: 57, anchor: 'middle' },
  ex_alu_to_pc: { x: 560, y: 41, anchor: 'middle' },
  mem_store_data: { x: 886, y: 370, anchor: 'start' },
  mem_alu_result: { x: 1076, y: 209, anchor: 'end' },
  wb_write_back: { x: 1166, y: 242, anchor: 'start' },
};

export interface PipelineLayout {
  readonly wires: Partial<Readonly<Record<PipelineWireId, Paths>>>;
  readonly labels: Partial<Readonly<Record<PipelineWireId, LabelSpot>>>;
  readonly muxes: Partial<Readonly<Record<PipelineMuxId, MuxShape>>>;
}

export function pipelineLayout(forwarding: boolean): PipelineLayout {
  if (forwarding) {
    return {
      wires: { ...COMMON_WIRES, ...FORWARD_WIRES },
      labels: {
        ...COMMON_LABELS,
        ex_operand_a: { x: 598, y: 225, anchor: 'start' },
        ex_operand_b: { x: 598, y: 315, anchor: 'start' },
      },
      muxes: { ...COMMON_MUXES, ...FORWARD_MUXES },
    };
  }
  return {
    wires: { ...COMMON_WIRES, ...DIRECT_WIRES },
    labels: {
      ...COMMON_LABELS,
      ex_operand_a: { x: 604, y: 209, anchor: 'start' },
      ex_operand_b: { x: 604, y: 299, anchor: 'start' },
    },
    muxes: COMMON_MUXES,
  };
}
