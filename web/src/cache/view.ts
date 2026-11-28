import type { CacheAccess, CacheKind, CacheOptions, CacheView, HistoryPoint } from '../core';
import { hexWord } from '../inspect/format';
import { plural } from '../run/stats';

// Lo que pinta el panel de la caché, calculado aparte del DOM para poder probarlo: opciones del
// formulario, partición de la dirección, celdas de cada conjunto y el gráfico de aciertos.

export const CACHE_KINDS: readonly CacheKind[] = ['instruction', 'data'];

export function cacheTitle(kind: CacheKind): string {
  return kind === 'instruction' ? 'Caché de instrucciones' : 'Caché de datos';
}

// Potencias de dos entre `min` y `max`, ambos incluidos.
export function powersOfTwo(min: number, max: number): number[] {
  const values: number[] = [];
  for (let value = Math.max(1, min); value <= max; value *= 2) {
    values.push(value);
  }
  return values;
}

export interface SizeChoices {
  readonly sets: readonly number[];
  readonly ways: readonly number[];
  readonly lineBytes: readonly number[];
}

export function sizeChoices(options: CacheOptions): SizeChoices {
  return {
    sets: powersOfTwo(1, options.max_sets),
    ways: powersOfTwo(1, options.max_ways),
    lineBytes: powersOfTwo(options.min_line_bytes, options.max_line_bytes),
  };
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 && bytes % 1024 === 0) {
    return `${String(bytes / 1024)} KiB`;
  }
  return plural(bytes, 'byte', 'bytes');
}

export function formatRate(rate: number | null): string {
  return rate === null ? '—' : `${(rate * 100).toFixed(1).replace('.', ',')} %`;
}

// Un trozo de la dirección: etiqueta, índice o desplazamiento.
export interface AddressField {
  readonly name: 'etiqueta' | 'índice' | 'desplazamiento';
  readonly bits: number;
  // Bits [high:low] de la dirección; `null` si el campo no tiene bits (un solo conjunto).
  readonly high: number | null;
  readonly low: number | null;
  // Valor del campo en la dirección dada; `null` sin dirección o sin bits.
  readonly value: string | null;
}

function fieldValue(bits: number, value: number): string {
  if (bits === 0) {
    return '—';
  }
  const digits = Math.max(1, Math.ceil(bits / 4));
  return `0x${value.toString(16).padStart(digits, '0')}`;
}

export function addressFields(
  view: Pick<CacheView, 'tag_bits' | 'index_bits' | 'offset_bits'>,
  address: number | null,
): AddressField[] {
  const layout: [AddressField['name'], number][] = [
    ['etiqueta', view.tag_bits],
    ['índice', view.index_bits],
    ['desplazamiento', view.offset_bits],
  ];
  let high = 31;
  return layout.map(([name, bits]) => {
    const low = high - bits + 1;
    const field: AddressField = {
      name,
      bits,
      high: bits === 0 ? null : high,
      low: bits === 0 ? null : low,
      value:
        address === null || bits === 0
          ? null
          : // Como mínimo hay 2 bits de desplazamiento, así que `bits` < 32 y la máscara cabe.
            fieldValue(bits, ((address >>> low) & ((1 << bits) - 1)) >>> 0),
    };
    high = low - 1;
    return field;
  });
}

export type RecentMark = 'hit' | 'miss';

export interface LineCellView {
  readonly valid: boolean;
  readonly dirty: boolean;
  readonly tag: string;
  readonly address: string;
  readonly age: string;
  // La vía que tocó el último paso, si fue esta.
  readonly recent: RecentMark | null;
  readonly description: string;
}

export interface SetRowView {
  readonly set: number;
  readonly lines: readonly LineCellView[];
  readonly recent: boolean;
}

function ageText(age: number | null, ways: number): string {
  if (age === null || ways === 1) {
    return '';
  }
  return age === 0 ? 'sale la siguiente' : `${String(age)} por delante`;
}

export function setRows(view: CacheView): SetRowView[] {
  const touched = new Map<string, RecentMark>();
  for (const access of view.recent) {
    if (access.way !== null) {
      touched.set(`${String(access.set)}:${String(access.way)}`, access.hit ? 'hit' : 'miss');
    }
  }
  return view.sets.map((lines, set) => {
    const cells = lines.map((line, way): LineCellView => {
      const recent = touched.get(`${String(set)}:${String(way)}`) ?? null;
      const where = `Conjunto ${String(set)}, vía ${String(way)}`;
      if (!line.valid) {
        return {
          valid: false,
          dirty: false,
          tag: '',
          address: '',
          age: '',
          recent,
          description: `${where}: vacía`,
        };
      }
      const address = line.address === null ? '' : hexWord(line.address);
      const age = ageText(line.age, view.settings.ways);
      const parts = [
        `${where}: etiqueta 0x${line.tag.toString(16)}`,
        `bloque ${address}`,
        line.dirty ? 'sucia' : 'limpia',
      ];
      if (age !== '') {
        parts.push(age);
      }
      if (recent !== null) {
        parts.push(recent === 'hit' ? 'acierto en el último paso' : 'cargada en el último paso');
      }
      return {
        valid: true,
        dirty: line.dirty,
        tag: `0x${line.tag.toString(16)}`,
        address,
        age,
        recent,
        description: parts.join(', '),
      };
    });
    return { set, lines: cells, recent: cells.some((cell) => cell.recent !== null) };
  });
}

// Frase de cada acceso del último paso.
export function describeAccess(access: CacheAccess): string {
  const op = access.op === 'read' ? 'Lectura' : 'Escritura';
  const where =
    access.way === null
      ? 'sin asignar línea, directa a memoria'
      : `conjunto ${String(access.set)}, vía ${String(access.way)}`;
  let text = `${op} de ${hexWord(access.address)}: ${access.hit ? 'acierto' : 'fallo'} · ${where}`;
  if (access.eviction !== null) {
    text += ` · expulsa el bloque ${hexWord(access.eviction.address)}${access.eviction.dirty ? ' (sucio: se escribe en memoria)' : ''}`;
  }
  return text;
}

export interface CounterView {
  readonly label: string;
  readonly value: string;
}

// La caché de instrucciones nunca recibe escrituras: sus contadores de escritura sobran.
export function counters(view: CacheView, kind: CacheKind): CounterView[] {
  const { stats } = view;
  const accesses = stats.reads + stats.writes;
  const hits = stats.read_hits + stats.write_hits;
  const list: CounterView[] = [
    { label: 'accesos', value: String(accesses) },
    { label: 'aciertos', value: String(hits) },
    { label: 'fallos', value: String(accesses - hits) },
    { label: 'tasa de acierto', value: formatRate(view.hit_rate) },
    { label: 'expulsiones', value: String(stats.evictions) },
  ];
  if (kind === 'data') {
    list.push(
      {
        label: 'aciertos en lecturas',
        value: `${String(stats.read_hits)} de ${String(stats.reads)}`,
      },
      {
        label: 'aciertos en escrituras',
        value: `${String(stats.write_hits)} de ${String(stats.writes)}`,
      },
      view.settings.write_policy === 'write-back'
        ? { label: 'bloques sucios escritos', value: String(stats.writebacks) }
        : { label: 'escrituras a memoria', value: String(stats.memory_writes) },
    );
  }
  if (stats.uncached > 0) {
    list.push({ label: 'E/S sin caché', value: String(stats.uncached) });
  }
  return list;
}

// Gráfico de la tasa de acierto acumulada frente al número de accesos, en unidades del viewBox.
export const CHART = { width: 320, height: 120, left: 34, right: 8, top: 8, bottom: 20 } as const;

export interface ChartPoint {
  readonly x: number;
  readonly y: number;
  readonly accesses: number;
  readonly rate: number;
}

export interface ChartView {
  readonly points: readonly ChartPoint[];
  readonly path: string;
  readonly maxAccesses: number;
  readonly description: string;
}

export function hitChart(history: readonly HistoryPoint[]): ChartView {
  const valid = history.filter((point) => point.accesses > 0);
  const last = valid.at(-1);
  if (last === undefined) {
    return { points: [], path: '', maxAccesses: 0, description: 'Todavía no hay accesos.' };
  }
  const plotWidth = CHART.width - CHART.left - CHART.right;
  const plotHeight = CHART.height - CHART.top - CHART.bottom;
  const maxAccesses = last.accesses;
  const points = valid.map((point): ChartPoint => {
    const rate = point.hits / point.accesses;
    return {
      x: CHART.left + (point.accesses / maxAccesses) * plotWidth,
      y: CHART.top + (1 - rate) * plotHeight,
      accesses: point.accesses,
      rate,
    };
  });
  const path = points
    .map((point, index) => `${index === 0 ? 'M' : 'L'}${point.x.toFixed(1)},${point.y.toFixed(1)}`)
    .join(' ');
  const rates = points.map((point) => point.rate);
  const description = `Tasa de acierto acumulada: ${formatRate(last.hits / last.accesses)} tras ${plural(maxAccesses, 'acceso', 'accesos')} (mínimo ${formatRate(Math.min(...rates))}, máximo ${formatRate(Math.max(...rates))}).`;
  return { points, path, maxAccesses, description };
}

// Punto del historial más cercano a la posición horizontal `x` del viewBox.
export function nearestPoint(chart: ChartView, x: number): ChartPoint | null {
  let best: ChartPoint | null = null;
  for (const point of chart.points) {
    if (best === null || Math.abs(point.x - x) < Math.abs(best.x - x)) {
      best = point;
    }
  }
  return best;
}
