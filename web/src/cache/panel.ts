import type { CacheKind, CacheOptions, CacheSettings, CacheView, CachesView } from '../core';
import { svg } from '../datapath/svg';
import {
  CACHE_KINDS,
  CHART,
  addressFields,
  cacheTitle,
  counters,
  describeAccess,
  formatBytes,
  formatRate,
  hitChart,
  nearestPoint,
  setRows,
  sizeChoices,
  type ChartView,
  type LineCellView,
} from './view';

export interface CacheElements {
  readonly section: HTMLElement;
  readonly body: HTMLElement;
}

export type ConfigureCache = (kind: CacheKind, settings: CacheSettings | null) => boolean;

interface Form {
  readonly enabled: HTMLInputElement;
  readonly sets: HTMLSelectElement;
  readonly ways: HTMLSelectElement;
  readonly lineBytes: HTMLSelectElement;
  readonly replacement: HTMLSelectElement;
  readonly writePolicy: HTMLSelectElement | null;
  readonly writeAllocate: HTMLInputElement | null;
}

// Tabla de conjuntos reutilizada entre repintados: con 1024 conjuntos × 16 vías rehacerla en
// cada fotograma de la ejecución continua se nota; solo se cambian las celdas que cambian.
interface SetsTable {
  readonly shape: string;
  readonly table: HTMLTableElement;
  readonly rows: HTMLTableRowElement[];
  readonly cells: HTMLTableCellElement[][];
  readonly keys: string[][];
}

interface Block {
  readonly kind: CacheKind;
  readonly root: HTMLElement;
  readonly form: Form;
  readonly details: HTMLElement;
  readonly geometry: HTMLElement;
  readonly address: HTMLElement;
  readonly recent: HTMLUListElement;
  readonly counters: HTMLDListElement;
  readonly chart: SVGSVGElement;
  readonly chartLine: SVGPathElement;
  readonly chartDot: SVGCircleElement;
  readonly chartCursor: SVGLineElement;
  readonly chartAxis: SVGTextElement;
  readonly chartReadout: HTMLElement;
  readonly chartCaption: HTMLElement;
  readonly setsScroll: HTMLElement;
  chartView: ChartView;
  sets: SetsTable | null;
}

let uid = 0;

function nextId(prefix: string): string {
  uid += 1;
  return `${prefix}-${String(uid)}`;
}

function select(
  labelText: string,
  choices: readonly { value: string; label: string }[],
  parent: HTMLElement,
): HTMLSelectElement {
  const wrapper = document.createElement('div');
  wrapper.className = 'cache-field';
  const id = nextId('cache-field');
  const label = document.createElement('label');
  label.className = 'label';
  label.htmlFor = id;
  label.textContent = labelText;
  const control = document.createElement('select');
  control.id = id;
  for (const choice of choices) {
    const option = document.createElement('option');
    option.value = choice.value;
    option.textContent = choice.label;
    control.append(option);
  }
  wrapper.append(label, control);
  parent.append(wrapper);
  return control;
}

function checkbox(labelText: string, parent: HTMLElement): HTMLInputElement {
  const label = document.createElement('label');
  label.className = 'cache-check label';
  const input = document.createElement('input');
  input.type = 'checkbox';
  label.append(input, ` ${labelText}`);
  parent.append(label);
  return input;
}

function numbers(values: readonly number[], unit: (value: number) => string) {
  return values.map((value) => ({ value: String(value), label: unit(value) }));
}

// Panel de las cachés. La configuración la lleva el formulario (empieza sin cachés, como la
// sesión) y cada cambio se aplica al momento: el núcleo reinicia el programa con ella.
export class CachePanel {
  readonly #options: CacheOptions;
  readonly #configure: ConfigureCache;
  readonly #onError: (error: unknown) => void;
  readonly #blocks: Block[];
  readonly #settings = new Map<CacheKind, CacheSettings | null>();
  #visible = false;
  #lastSource: (() => CachesView) | null = null;

  constructor(
    elements: CacheElements,
    options: CacheOptions,
    configure: ConfigureCache,
    onError: (error: unknown) => void,
  ) {
    this.#options = options;
    this.#configure = configure;
    this.#onError = onError;
    this.#blocks = CACHE_KINDS.map((kind) => this.#createBlock(kind));
    elements.body.replaceChildren(...this.#blocks.map((block) => block.root));
    for (const block of this.#blocks) {
      this.#settings.set(block.kind, null);
      this.#showForm(block, null);
    }
    // Las cachés solo se piden al núcleo con el panel a la vista: con muchas líneas pesan.
    const observer = new IntersectionObserver((entries) => {
      const visible = entries.some((entry) => entry.isIntersecting);
      if (visible && !this.#visible) {
        this.#visible = true;
        if (this.#lastSource !== null) {
          this.render(this.#lastSource);
        }
      } else {
        this.#visible = visible;
      }
    });
    observer.observe(elements.section);
  }

  // `source` es `null` sin programa ensamblado; si no, se llama solo si hace falta.
  render(source: (() => CachesView) | null): void {
    this.#lastSource = source;
    const anyEnabled = [...this.#settings.values()].some((settings) => settings !== null);
    if (source === null || !anyEnabled) {
      for (const block of this.#blocks) {
        this.#renderBlock(block, null);
      }
      return;
    }
    if (!this.#visible) {
      return;
    }
    let views: CachesView;
    try {
      views = source();
    } catch (error: unknown) {
      this.#onError(error);
      return;
    }
    for (const block of this.#blocks) {
      this.#renderBlock(block, views[block.kind]);
    }
  }

  #createBlock(kind: CacheKind): Block {
    const root = document.createElement('section');
    root.className = 'cache-block';
    root.dataset.kind = kind;
    const titleId = nextId('cache-title');
    root.setAttribute('aria-labelledby', titleId);
    const head = document.createElement('div');
    head.className = 'cache-block-head';
    const title = document.createElement('h3');
    title.className = 'label';
    title.id = titleId;
    title.textContent = cacheTitle(kind);
    const formElement = document.createElement('div');
    formElement.className = 'cache-form';
    formElement.setAttribute('role', 'group');
    formElement.setAttribute('aria-label', `Configuración de la ${cacheTitle(kind).toLowerCase()}`);
    const enabled = checkbox('activar', head);
    head.prepend(title);
    const choices = sizeChoices(this.#options);
    const sets = select(
      'Conjuntos',
      numbers(choices.sets, (value) => String(value)),
      formElement,
    );
    const ways = select(
      'Vías',
      numbers(choices.ways, (value) => (value === 1 ? '1 (directa)' : String(value))),
      formElement,
    );
    const lineBytes = select('Línea', numbers(choices.lineBytes, formatBytes), formElement);
    const replacement = select(
      'Reemplazo',
      this.#options.replacements.map((choice) => ({ value: choice.id, label: choice.label })),
      formElement,
    );
    // La caché de instrucciones nunca recibe escrituras: sus políticas no cambiarían nada.
    let writePolicy: HTMLSelectElement | null = null;
    let writeAllocate: HTMLInputElement | null = null;
    if (kind === 'data') {
      writePolicy = select(
        'Escritura',
        this.#options.write_policies.map((choice) => ({ value: choice.id, label: choice.label })),
        formElement,
      );
      writeAllocate = checkbox('asignar en fallo de escritura', formElement);
    }
    const form: Form = { enabled, sets, ways, lineBytes, replacement, writePolicy, writeAllocate };

    const details = document.createElement('div');
    details.className = 'cache-details';
    const geometry = document.createElement('p');
    geometry.className = 'cache-geometry';
    const address = document.createElement('div');
    address.className = 'cache-address';
    address.setAttribute('role', 'group');
    address.setAttribute('aria-label', 'Partes de la dirección');
    const recentTitle = document.createElement('h4');
    recentTitle.className = 'label';
    recentTitle.textContent = 'Último paso';
    const recent = document.createElement('ul');
    recent.className = 'cache-recent';
    const countersList = document.createElement('dl');
    countersList.className = 'cache-counters';

    const figure = document.createElement('figure');
    figure.className = 'cache-chart';
    const chart = svg('svg', {
      viewBox: `0 0 ${String(CHART.width)} ${String(CHART.height)}`,
      role: 'img',
      'aria-hidden': 'true',
    });
    const bottom = CHART.height - CHART.bottom;
    for (const [rate, text] of [
      [1, '100 %'],
      [0.5, '50 %'],
      [0, '0 %'],
    ] as const) {
      const y = CHART.top + (1 - rate) * (bottom - CHART.top);
      svg(
        'line',
        { x1: CHART.left, x2: CHART.width - CHART.right, y1: y, y2: y, class: 'cache-grid' },
        chart,
      );
      const label = svg(
        'text',
        { x: CHART.left - 4, y: y + 3, 'text-anchor': 'end', class: 'cache-tick' },
        chart,
      );
      label.textContent = text;
    }
    const axisLabel = svg(
      'text',
      {
        x: CHART.width - CHART.right,
        y: CHART.height - 4,
        'text-anchor': 'end',
        class: 'cache-tick',
      },
      chart,
    );
    const zero = svg(
      'text',
      { x: CHART.left, y: CHART.height - 4, 'text-anchor': 'start', class: 'cache-tick' },
      chart,
    );
    zero.textContent = '0 accesos';
    const cursor = svg('line', { y1: CHART.top, y2: bottom, class: 'cache-cursor' }, chart);
    cursor.style.display = 'none';
    const line = svg('path', { class: 'cache-line', d: '' }, chart);
    const dot = svg('circle', { r: 3.5, class: 'cache-dot' }, chart);
    dot.style.display = 'none';
    const readout = document.createElement('p');
    readout.className = 'cache-readout label';
    readout.setAttribute('aria-hidden', 'true');
    const caption = document.createElement('figcaption');
    caption.className = 'datapath-caption';
    figure.append(chart, readout, caption);

    const setsTitle = document.createElement('h4');
    setsTitle.className = 'label';
    setsTitle.textContent = 'Conjuntos';
    const setsScroll = document.createElement('div');
    setsScroll.className = 'cache-sets-scroll';
    setsScroll.tabIndex = 0;
    setsScroll.setAttribute('role', 'group');
    setsScroll.setAttribute('aria-label', `Conjuntos de la ${cacheTitle(kind).toLowerCase()}`);

    const summary = document.createElement('div');
    summary.className = 'cache-summary';
    const recentBox = document.createElement('div');
    recentBox.append(recentTitle, recent);
    recentBox.className = 'cache-recent-box';
    summary.append(address, recentBox, countersList, figure);
    details.append(geometry, summary, setsTitle, setsScroll);
    root.append(head, formElement, details);

    const block: Block = {
      kind,
      root,
      form,
      details,
      geometry,
      address,
      recent,
      counters: countersList,
      chart,
      chartLine: line,
      chartDot: dot,
      chartCursor: cursor,
      chartAxis: axisLabel,
      chartReadout: readout,
      chartCaption: caption,
      setsScroll,
      chartView: hitChart([]),
      sets: null,
    };
    for (const control of [
      enabled,
      sets,
      ways,
      lineBytes,
      replacement,
      writePolicy,
      writeAllocate,
    ]) {
      control?.addEventListener('change', () => {
        this.#apply(block, control === writePolicy);
      });
    }
    chart.addEventListener('pointermove', (event) => {
      this.#hover(block, event);
    });
    chart.addEventListener('pointerleave', () => {
      block.chartCursor.style.display = 'none';
      block.chartReadout.textContent = '';
    });
    return block;
  }

  #readForm(block: Block, policyChanged: boolean): CacheSettings | null {
    const { form } = block;
    if (!form.enabled.checked) {
      return null;
    }
    const previous = this.#settings.get(block.kind) ?? this.#options.default;
    const writePolicy = form.writePolicy?.value ?? previous.write_policy;
    // Al cambiar de política se propone su pareja de los libros (ADR 0004); luego se respeta lo
    // que se marque a mano.
    const writeAllocate = policyChanged
      ? writePolicy === 'write-back'
      : (form.writeAllocate?.checked ?? previous.write_allocate);
    return {
      sets: Number(form.sets.value),
      ways: Number(form.ways.value),
      line_bytes: Number(form.lineBytes.value),
      replacement: form.replacement.value,
      write_policy: writePolicy,
      write_allocate: writeAllocate,
    };
  }

  #apply(block: Block, policyChanged: boolean): void {
    const settings = this.#readForm(block, policyChanged);
    if (this.#configure(block.kind, settings)) {
      this.#settings.set(block.kind, settings);
    }
    this.#showForm(block, this.#settings.get(block.kind) ?? null);
    if (this.#lastSource !== null) {
      this.render(this.#lastSource);
    }
  }

  #showForm(block: Block, settings: CacheSettings | null): void {
    const { form } = block;
    const shown = settings ?? this.#options.default;
    form.enabled.checked = settings !== null;
    form.sets.value = String(shown.sets);
    form.ways.value = String(shown.ways);
    form.lineBytes.value = String(shown.line_bytes);
    form.replacement.value = shown.replacement;
    if (form.writePolicy !== null) {
      form.writePolicy.value = shown.write_policy;
    }
    if (form.writeAllocate !== null) {
      form.writeAllocate.checked = shown.write_allocate;
    }
    for (const control of [
      form.sets,
      form.ways,
      form.lineBytes,
      form.replacement,
      form.writePolicy,
      form.writeAllocate,
    ]) {
      if (control !== null) {
        control.disabled = settings === null;
      }
    }
    block.root.dataset.enabled = String(settings !== null);
  }

  #renderBlock(block: Block, view: CacheView | null): void {
    block.details.hidden = view === null;
    if (view === null) {
      return;
    }
    const { settings } = view;
    block.geometry.textContent = `${formatBytes(view.size_bytes)} · ${String(settings.sets)} ${settings.sets === 1 ? 'conjunto' : 'conjuntos'} × ${String(settings.ways)} ${settings.ways === 1 ? 'vía' : 'vías'} × ${formatBytes(settings.line_bytes)}`;
    this.#renderAddress(block, view);
    this.#renderRecent(block, view);
    block.counters.replaceChildren(
      ...counters(view, block.kind).flatMap((counter) => {
        const term = document.createElement('dt');
        term.className = 'label';
        term.textContent = counter.label;
        const value = document.createElement('dd');
        value.textContent = counter.value;
        return [term, value];
      }),
    );
    this.#renderChart(block, view);
    this.#renderSets(block, view);
  }

  #renderAddress(block: Block, view: CacheView): void {
    const [last] = view.recent.slice(-1);
    const fields = addressFields(view, last?.address ?? null);
    block.address.replaceChildren(
      ...fields.map((field) => {
        const cell = document.createElement('div');
        cell.className = 'cache-address-field';
        cell.dataset.empty = String(field.bits === 0);
        // El ancho sigue a los bits para que la proporción se lea de un vistazo.
        cell.style.flexGrow = String(Math.max(field.bits, 3));
        const name = document.createElement('span');
        name.className = 'label';
        name.textContent = field.name;
        const range = document.createElement('span');
        range.className = 'cache-address-bits';
        range.textContent =
          field.high === null
            ? 'sin bits'
            : `${String(field.bits)} ${field.bits === 1 ? 'bit' : 'bits'} · [${String(field.high)}:${String(field.low)}]`;
        const value = document.createElement('span');
        value.className = 'cache-address-value';
        value.textContent = field.value ?? '';
        cell.append(name, range, value);
        return cell;
      }),
    );
  }

  #renderRecent(block: Block, view: CacheView): void {
    if (view.recent.length === 0) {
      const item = document.createElement('li');
      item.className = 'cache-recent-none';
      item.textContent = 'sin accesos en el último paso';
      block.recent.replaceChildren(item);
      return;
    }
    block.recent.replaceChildren(
      ...view.recent.map((access) => {
        const item = document.createElement('li');
        item.dataset.hit = String(access.hit);
        item.textContent = describeAccess(access);
        return item;
      }),
    );
  }

  #renderChart(block: Block, view: CacheView): void {
    const chart = hitChart(view.history);
    block.chartView = chart;
    block.chartLine.setAttribute('d', chart.path);
    const last = chart.points.at(-1);
    if (last === undefined) {
      block.chartDot.style.display = 'none';
    } else {
      block.chartDot.style.display = '';
      block.chartDot.setAttribute('cx', last.x.toFixed(1));
      block.chartDot.setAttribute('cy', last.y.toFixed(1));
    }
    block.chartAxis.textContent =
      chart.maxAccesses === 0 ? '' : `${String(chart.maxAccesses)} accesos`;
    block.chartCaption.textContent = chart.description;
  }

  #hover(block: Block, event: PointerEvent): void {
    const box = block.chart.getBoundingClientRect();
    if (box.width === 0) {
      return;
    }
    const x = ((event.clientX - box.left) / box.width) * CHART.width;
    const point = nearestPoint(block.chartView, x);
    if (point === null) {
      return;
    }
    block.chartCursor.style.display = '';
    block.chartCursor.setAttribute('x1', point.x.toFixed(1));
    block.chartCursor.setAttribute('x2', point.x.toFixed(1));
    block.chartReadout.textContent = `${String(point.accesses)} accesos · ${formatRate(point.rate)}`;
  }

  #renderSets(block: Block, view: CacheView): void {
    const { sets, ways } = view.settings;
    const shape = `${String(sets)}×${String(ways)}`;
    if (block.sets?.shape !== shape) {
      block.sets = buildSetsTable(shape, sets, ways);
      block.setsScroll.replaceChildren(block.sets.table);
    }
    const table = block.sets;
    const rows = setRows(view);
    for (const row of rows) {
      table.rows[row.set]?.toggleAttribute('data-recent', row.recent);
      row.lines.forEach((line, way) => {
        const key = cellKey(line);
        const keys = table.keys[row.set];
        const cell = table.cells[row.set]?.[way];
        if (keys === undefined || cell === undefined || keys[way] === key) {
          return;
        }
        keys[way] = key;
        fillLineCell(cell, line);
      });
    }
  }
}

function cellKey(line: LineCellView): string {
  return `${line.description}|${line.recent ?? ''}`;
}

function buildSetsTable(shape: string, sets: number, ways: number): SetsTable {
  const table = document.createElement('table');
  table.className = 'cache-sets';
  const caption = document.createElement('caption');
  caption.className = 'visually-hidden';
  caption.textContent = 'Líneas de cada conjunto: válida, etiqueta, bloque y edad';
  const head = document.createElement('thead');
  const headRow = head.insertRow();
  const corner = document.createElement('th');
  corner.scope = 'col';
  corner.textContent = 'Conjunto';
  headRow.append(corner);
  for (let way = 0; way < ways; way += 1) {
    const th = document.createElement('th');
    th.scope = 'col';
    th.textContent = `Vía ${String(way)}`;
    headRow.append(th);
  }
  const body = document.createElement('tbody');
  const rows: HTMLTableRowElement[] = [];
  const cells: HTMLTableCellElement[][] = [];
  const keys: string[][] = [];
  for (let set = 0; set < sets; set += 1) {
    const tr = body.insertRow();
    const th = document.createElement('th');
    th.scope = 'row';
    th.textContent = String(set);
    tr.append(th);
    const rowCells: HTMLTableCellElement[] = [];
    for (let way = 0; way < ways; way += 1) {
      const td = document.createElement('td');
      tr.append(td);
      rowCells.push(td);
    }
    rows.push(tr);
    cells.push(rowCells);
    keys.push(rowCells.map(() => ''));
  }
  table.append(caption, head, body);
  return { shape, table, rows, cells, keys };
}

function fillLineCell(cell: HTMLTableCellElement, line: LineCellView): void {
  cell.dataset.valid = String(line.valid);
  cell.dataset.dirty = String(line.dirty);
  if (line.recent === null) {
    delete cell.dataset.recent;
  } else {
    cell.dataset.recent = line.recent;
  }
  cell.title = line.description;
  const visible = document.createElement('span');
  visible.setAttribute('aria-hidden', 'true');
  if (line.valid) {
    const tag = document.createElement('span');
    tag.className = 'cache-tag';
    tag.textContent = line.tag + (line.dirty ? ' · S' : '');
    const address = document.createElement('span');
    address.className = 'cache-line-address';
    address.textContent = line.address;
    visible.append(tag, address);
    if (line.age !== '') {
      const age = document.createElement('span');
      age.className = 'cache-age';
      age.textContent = line.age;
      visible.append(age);
    }
  } else {
    visible.textContent = '·';
  }
  const spoken = document.createElement('span');
  spoken.className = 'visually-hidden';
  spoken.textContent = line.description;
  cell.replaceChildren(visible, spoken);
}
