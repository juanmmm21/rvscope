import type { MuxId, WireId } from './single';
import type { Box, LabelSpot, MuxShape, Point } from './svg';

// Geometría del diagrama del procesador de un ciclo, en unidades del `viewBox`. El dibujo es
// propio de rvscope: el flujo va de izquierda a derecha (búsqueda, decodificación, ALU,
// memoria y escritura) y las vueltas al PC y al banco de registros rodean el diagrama por
// arriba y por abajo para no cruzar las piezas.

export const VIEW_BOX = { width: 1020, height: 450 } as const;

export const MUXES: Readonly<Record<MuxId, MuxShape>> = {
  pc: {
    x: 24,
    y: 180,
    width: 22,
    height: 100,
    inputs: [200, 230, 260],
    output: 230,
    title: 'Siguiente PC',
  },
  alu_a: {
    x: 600,
    y: 175,
    width: 22,
    height: 80,
    inputs: [190, 215, 240],
    output: 215,
    title: 'Operando A',
  },
  alu_b: {
    x: 600,
    y: 260,
    width: 22,
    height: 55,
    inputs: [272, 300],
    output: 287,
    title: 'Operando B',
  },
  write_back: {
    x: 960,
    y: 200,
    width: 22,
    height: 90,
    inputs: [215, 245, 275],
    output: 245,
    title: 'Dato para rd',
  },
};

export const BOXES = {
  pc: { x: 70, y: 205, width: 86, height: 50 },
  adder: { x: 190, y: 50, width: 56, height: 50 },
  instructions: { x: 190, y: 180, width: 116, height: 110 },
  control: { x: 340, y: 40, width: 215, height: 100 },
  registers: { x: 350, y: 165, width: 130, height: 150 },
  immediate: { x: 360, y: 345, width: 120, height: 40 },
  target: { x: 660, y: 55, width: 64, height: 60 },
  data: { x: 760, y: 190, width: 130, height: 130 },
} as const satisfies Readonly<Record<string, Box>>;

// Silueta de la ALU: trapecio con la muesca de entrada.
export const ALU_OUTLINE: readonly Point[] = [
  [660, 175],
  [724, 205],
  [724, 285],
  [660, 315],
  [660, 262],
  [672, 245],
  [660, 228],
];

// Cada cable es una o varias polilíneas; los que comparten tramo con otro lo dibujan entero
// para que, al encenderse, se vea el camino completo.
export const WIRES: Readonly<Record<WireId, readonly (readonly Point[])[]>> = {
  next_pc: [
    [
      [46, 230],
      [70, 230],
    ],
  ],
  pc: [
    [
      [156, 230],
      [190, 230],
    ],
    [
      [170, 230],
      [170, 75],
      [190, 75],
    ],
  ],
  pc_plus_4: [
    [
      [246, 75],
      [262, 75],
      [262, 32],
      [16, 32],
      [16, 200],
      [24, 200],
    ],
  ],
  instruction: [
    [
      [306, 230],
      [322, 230],
      [322, 95],
      [340, 95],
    ],
    [
      [322, 230],
      [322, 365],
      [360, 365],
    ],
  ],
  rs1_index: [
    [
      [322, 190],
      [350, 190],
    ],
  ],
  rs2_index: [
    [
      [322, 220],
      [350, 220],
    ],
  ],
  rd_index: [
    [
      [322, 250],
      [350, 250],
    ],
  ],
  rs1: [
    [
      [480, 190],
      [600, 190],
    ],
  ],
  rs2: [
    [
      [480, 272],
      [600, 272],
    ],
  ],
  store_data: [
    [
      [480, 272],
      [520, 272],
      [520, 348],
      [740, 348],
      [740, 305],
      [760, 305],
    ],
  ],
  immediate: [
    [
      [480, 365],
      [560, 365],
      [560, 300],
      [600, 300],
    ],
  ],
  immediate_to_target: [
    [
      [480, 365],
      [560, 365],
      [560, 100],
      [660, 100],
    ],
  ],
  pc_to_alu: [
    [
      [170, 230],
      [170, 410],
      [575, 410],
      [575, 215],
      [600, 215],
    ],
  ],
  pc_to_target: [
    [
      [170, 230],
      [170, 410],
      [575, 410],
      [575, 80],
      [660, 80],
    ],
  ],
  zero: [
    [
      [586, 240],
      [600, 240],
    ],
  ],
  alu_a: [
    [
      [622, 215],
      [660, 215],
    ],
  ],
  alu_b: [
    [
      [622, 287],
      [660, 287],
    ],
  ],
  address: [
    [
      [724, 245],
      [760, 245],
    ],
  ],
  alu_to_write_back: [
    [
      [724, 245],
      [745, 245],
      [745, 150],
      [930, 150],
      [930, 215],
      [960, 215],
    ],
  ],
  alu_to_pc: [
    [
      [724, 245],
      [745, 245],
      [745, 440],
      [10, 440],
      [10, 260],
      [24, 260],
    ],
  ],
  target: [
    [
      [724, 85],
      [740, 85],
      [740, 14],
      [4, 14],
      [4, 230],
      [24, 230],
    ],
  ],
  memory_read: [
    [
      [890, 245],
      [960, 245],
    ],
  ],
  link: [
    [
      [246, 75],
      [262, 75],
      [262, 32],
      [945, 32],
      [945, 275],
      [960, 275],
    ],
  ],
  write_back: [
    [
      [982, 245],
      [1010, 245],
      [1010, 425],
      [330, 425],
      [330, 290],
      [350, 290],
    ],
  ],
};

// Dónde va el valor de los cables que llevan rótulo.
export const WIRE_LABELS: Partial<Readonly<Record<WireId, LabelSpot>>> = {
  next_pc: { x: 22, y: 300, anchor: 'start' },
  pc_plus_4: { x: 140, y: 27, anchor: 'middle' },
  rs1: { x: 488, y: 184, anchor: 'start' },
  rs2: { x: 488, y: 266, anchor: 'start' },
  store_data: { x: 488, y: 266, anchor: 'start' },
  immediate: { x: 488, y: 359, anchor: 'start' },
  immediate_to_target: { x: 488, y: 359, anchor: 'start' },
  target: { x: 380, y: 9, anchor: 'middle' },
  write_back: { x: 680, y: 419, anchor: 'middle' },
};
