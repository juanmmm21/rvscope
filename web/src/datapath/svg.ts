// Piezas de dibujo comunes a los diagramas de la ruta de datos: formas en unidades del `viewBox`
// y ayudantes para crear el SVG una sola vez.

export type Point = readonly [number, number];

export interface Box {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface MuxShape extends Box {
  // Altura de cada entrada, de arriba abajo, y de la salida.
  readonly inputs: readonly number[];
  readonly output: number;
  readonly title: string;
}

export interface LabelSpot {
  readonly x: number;
  readonly y: number;
  readonly anchor: 'start' | 'middle' | 'end';
}

const SVG = 'http://www.w3.org/2000/svg';

type SvgTag = keyof SVGElementTagNameMap;

export function svg<K extends SvgTag>(
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

export function points(path: readonly Point[]): string {
  return path.map(([x, y]) => `${String(x)},${String(y)}`).join(' ');
}

export function text(
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

export function box(parent: SVGElement, shape: Box, title: string): SVGGElement {
  const group = svg('g', { class: 'dp-unit' }, parent);
  svg('rect', { x: shape.x, y: shape.y, width: shape.width, height: shape.height, rx: 2 }, group);
  text(group, shape.x + shape.width / 2, shape.y + 13, 'dp-title', 'middle', title);
  return group;
}

export interface MuxParts {
  readonly group: SVGGElement;
  readonly selection: SVGPolylineElement;
}

export function mux(parent: SVGElement, shape: MuxShape): MuxParts {
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

// Raya que une la entrada elegida con la salida; vacía si el multiplexor no se usa.
export function selectInput(parts: MuxParts, shape: MuxShape, selected: number | null): void {
  const input = selected === null ? undefined : shape.inputs[selected];
  parts.selection.setAttribute(
    'points',
    input === undefined
      ? ''
      : points([
          [shape.x, input],
          [shape.x + shape.width, shape.output],
        ]),
  );
}

export interface WireParts {
  readonly group: SVGGElement;
  readonly label: SVGTextElement | null;
}

export function wire(
  parent: SVGElement,
  id: string,
  paths: readonly (readonly Point[])[],
  spot: LabelSpot | undefined,
): WireParts {
  const group = svg('g', { class: 'dp-wire', 'data-wire': id }, parent);
  for (const path of paths) {
    svg('polyline', { points: points(path) }, group);
  }
  const label = spot === undefined ? null : text(group, spot.x, spot.y, 'dp-value', spot.anchor);
  return { group, label };
}

// Los cables en uso pasan a un grupo de encima para que un tramo compartido con un cable
// apagado se vea encendido.
export function showWire(
  parts: WireParts,
  active: boolean,
  label: string,
  layers: { readonly idle: SVGGElement; readonly active: SVGGElement },
): void {
  const target = active ? layers.active : layers.idle;
  if (parts.group.parentNode !== target) {
    target.append(parts.group);
  }
  parts.group.dataset.active = String(active);
  if (parts.label !== null) {
    parts.label.textContent = label;
  }
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
