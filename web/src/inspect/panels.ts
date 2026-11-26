import type { Region, RegisterInfo, Snapshot } from '../core';
import { VALUE_FORMATS, formatValue, type ValueFormat } from './format';
import {
  PAGE_BYTES,
  ROW_BYTES,
  clampPage,
  lastDataAccesses,
  memoryRows,
  pageFor,
  parseAddress,
  regionAt,
  type AddressContext,
  type MemoryUnit,
} from './memory';

// Paneles de registros y memoria. Pintan lo que devuelve el núcleo; no calculan nada del
// procesador.

// Grupo de opciones excluyentes como radios: el teclado (flechas) y los lectores de pantalla
// ya saben manejarlos sin código propio.
function radioGroup<T extends string>(
  parent: HTMLElement,
  name: string,
  options: readonly { readonly id: T; readonly label: string; readonly description: string }[],
  selected: T,
  onChange: (id: T) => void,
): Map<T, HTMLInputElement> {
  const inputs = new Map<T, HTMLInputElement>();
  const items = options.map((option) => {
    const label = document.createElement('label');
    label.className = 'segment';
    label.title = option.description;
    const input = document.createElement('input');
    input.type = 'radio';
    input.name = name;
    input.value = option.id;
    input.checked = option.id === selected;
    input.addEventListener('change', () => {
      if (input.checked) {
        onChange(option.id);
      }
    });
    const text = document.createElement('span');
    text.textContent = option.label;
    label.append(input, text);
    inputs.set(option.id, input);
    return label;
  });
  // La `legend` del fieldset se queda: solo se cambian las opciones.
  for (const old of parent.querySelectorAll('.segment')) {
    old.remove();
  }
  parent.append(...items);
  return inputs;
}

export class RegisterPanel {
  readonly #values: HTMLElement[] = [];
  readonly #rows: HTMLElement[] = [];
  #format: ValueFormat = 'hex';
  #last: Snapshot | null = null;

  constructor(target: HTMLElement, formats: HTMLElement, registers: readonly RegisterInfo[]) {
    radioGroup(formats, 'register-format', VALUE_FORMATS, this.#format, (format) => {
      this.#format = format;
      target.dataset.format = format;
      this.render(this.#last);
    });
    target.dataset.format = this.#format;
    const rows = registers.map((info) => {
      const row = document.createElement('div');
      row.className = 'register';
      row.dataset.register = info.abi;
      const name = document.createElement('span');
      name.className = 'register-name';
      name.textContent = info.abi;
      name.title = `${info.name} · ${info.role}`;
      const value = document.createElement('span');
      value.className = 'register-value';
      row.append(name, value);
      this.#rows[info.index] = row;
      this.#values[info.index] = value;
      return row;
    });
    target.replaceChildren(...rows);
    this.render(null);
  }

  // Sin programa ensamblado se enseñan a cero y sin resaltar.
  render(snapshot: Snapshot | null): void {
    this.#last = snapshot;
    const changed = new Set(snapshot?.changed ?? []);
    this.#values.forEach((value, index) => {
      const text = formatValue(snapshot?.registers[index] ?? 0, this.#format);
      if (value.textContent !== text) {
        value.textContent = text;
      }
      const row = this.#rows[index];
      if (row !== undefined) {
        row.dataset.changed = String(changed.has(index));
      }
    });
  }
}

export interface MemorySource {
  regions(): readonly Region[];
  readMemory(address: number, length: number): Uint8Array;
}

const REGION_NAMES: Record<Region['kind'], string> = {
  text: 'Texto',
  data: 'Datos',
  stack: 'Pila',
  io: 'E/S',
};

const UNITS: readonly { id: MemoryUnit; label: string; description: string }[] = [
  { id: 'word', label: 'Palabras', description: 'Palabras de 32 bits (little-endian)' },
  { id: 'byte', label: 'Bytes', description: 'Byte a byte' },
];

// Índice de `sp` en el banco de registros.
const SP = 2;

export interface MemoryElements {
  readonly regions: HTMLElement;
  readonly units: HTMLElement;
  readonly gotoForm: HTMLFormElement;
  readonly gotoInput: HTMLInputElement;
  readonly gotoButton: HTMLButtonElement;
  readonly previous: HTMLButtonElement;
  readonly next: HTMLButtonElement;
  readonly followSp: HTMLInputElement;
  readonly status: HTMLElement;
  readonly grid: HTMLTableElement;
}

export class MemoryPanel {
  readonly #ui: MemoryElements;
  readonly #onError: (error: unknown) => void;
  #source: MemorySource | null = null;
  #regions: readonly Region[] = [];
  #regionInputs = new Map<Region['kind'], HTMLInputElement>();
  #unit: MemoryUnit = 'word';
  #page = 0;
  #followSp = true;
  #loadedOnce = false;
  #target: number | null = null;
  #message: string | null = null;
  #snapshot: Snapshot | null = null;
  #context: AddressContext | null = null;

  constructor(ui: MemoryElements, onError: (error: unknown) => void) {
    this.#ui = ui;
    this.#onError = onError;
    radioGroup(ui.units, 'memory-unit', UNITS, this.#unit, (unit) => {
      this.#unit = unit;
      this.#paint();
    });
    ui.gotoForm.addEventListener('submit', (event) => {
      event.preventDefault();
      this.#goto(ui.gotoInput.value);
    });
    ui.previous.addEventListener('click', () => {
      this.#move(-PAGE_BYTES);
    });
    ui.next.addEventListener('click', () => {
      this.#move(PAGE_BYTES);
    });
    ui.followSp.addEventListener('change', () => {
      this.#followSp = ui.followSp.checked;
      this.#target = null;
      this.#paint();
    });
    ui.grid.addEventListener('keydown', (event) => {
      const steps: Record<string, number> = {
        PageUp: -PAGE_BYTES,
        PageDown: PAGE_BYTES,
        ArrowUp: -ROW_BYTES,
        ArrowDown: ROW_BYTES,
      };
      const delta = steps[event.key];
      if (delta !== undefined && !event.ctrlKey && !event.metaKey && !event.altKey) {
        event.preventDefault();
        this.#move(delta);
      }
    });
    this.clear();
  }

  // Programa recién ensamblado: los segmentos pueden haber cambiado (el de texto mide lo que
  // mide el código). Como se ensambla al dejar de escribir, la posición se conserva; solo la
  // primera vez se empieza en la pila siguiendo `sp`, que es donde pasa casi todo.
  load(source: MemorySource): void {
    this.#source = source;
    this.#regions = source.regions();
    this.#message = null;
    this.#regionInputs = radioGroup(
      this.#ui.regions,
      'memory-region',
      this.#regions.map((region) => ({
        id: region.kind,
        label: REGION_NAMES[region.kind],
        description: `${REGION_NAMES[region.kind]}: desde 0x${region.base.toString(16).padStart(8, '0')}`,
      })),
      'stack',
      (kind) => {
        this.#showRegion(kind);
      },
    );
    if (!this.#loadedOnce) {
      this.#loadedOnce = true;
      this.#followSp = this.#regions.some((region) => region.kind === 'stack');
    }
    this.#setDisabled(false);
  }

  clear(): void {
    this.#source = null;
    this.#snapshot = null;
    this.#regions = [];
    this.#regionInputs = new Map();
    for (const old of this.#ui.regions.querySelectorAll('.segment')) {
      old.remove();
    }
    this.#ui.grid.replaceChildren();
    this.#ui.status.dataset.state = 'idle';
    this.#ui.status.textContent = 'sin programa ensamblado';
    this.#setDisabled(true);
  }

  render(snapshot: Snapshot, context: AddressContext): void {
    this.#snapshot = snapshot;
    this.#context = context;
    this.#paint();
  }

  #setDisabled(disabled: boolean): void {
    const ui = this.#ui;
    for (const control of [ui.gotoInput, ui.gotoButton, ui.previous, ui.next, ui.followSp]) {
      control.disabled = disabled;
    }
    for (const input of ui.units.querySelectorAll('input')) {
      input.disabled = disabled;
    }
  }

  #showRegion(kind: Region['kind']): void {
    const region = this.#regions.find((candidate) => candidate.kind === kind);
    if (region === undefined) {
      return;
    }
    this.#target = null;
    this.#message = null;
    this.#followSp = kind === 'stack';
    if (!this.#followSp) {
      this.#page = pageFor(region.base, region);
    }
    this.#paint();
  }

  #move(delta: number): void {
    if (this.#source === null) {
      return;
    }
    this.#followSp = false;
    this.#message = null;
    this.#page = clampPage(this.#page + delta);
    this.#paint();
  }

  #goto(text: string): void {
    if (this.#source === null || this.#context === null) {
      return;
    }
    const parsed = parseAddress(text, this.#context);
    if (!parsed.ok) {
      this.#message = parsed.message;
      this.#paint();
      return;
    }
    this.#message = null;
    this.#followSp = false;
    this.#target = parsed.address;
    this.#page = pageFor(parsed.address, regionAt(parsed.address, this.#regions));
    this.#paint();
  }

  #paint(): void {
    const source = this.#source;
    const snapshot = this.#snapshot;
    if (source === null || snapshot === null) {
      return;
    }
    const sp = snapshot.registers[SP] ?? 0;
    if (this.#followSp) {
      this.#page = pageFor(sp, regionAt(sp, this.#regions));
    }
    let bytes: Uint8Array;
    try {
      bytes = source.readMemory(this.#page, PAGE_BYTES);
    } catch (error: unknown) {
      this.#onError(error);
      return;
    }
    const rows = memoryRows(bytes, this.#page, this.#unit, {
      accesses: lastDataAccesses(snapshot),
      target: this.#target,
      sp,
    });
    this.#paintGrid(rows);
    this.#reveal(rows);

    const ui = this.#ui;
    const region = regionAt(this.#page, this.#regions);
    for (const [kind, input] of this.#regionInputs) {
      input.checked = region?.kind === kind;
    }
    ui.followSp.checked = this.#followSp;
    const end = this.#page + PAGE_BYTES - 1;
    const range = `0x${this.#page.toString(16).padStart(8, '0')} – 0x${end.toString(16).padStart(8, '0')}`;
    if (this.#message !== null) {
      ui.status.dataset.state = 'error';
      ui.status.textContent = this.#message;
    } else {
      ui.status.dataset.state = 'idle';
      ui.status.textContent =
        region === null
          ? `${range} · fuera del mapa de memoria`
          : `${REGION_NAMES[region.kind]} · ${range}`;
    }
  }

  #paintGrid(rows: ReturnType<typeof memoryRows>): void {
    const grid = this.#ui.grid;
    const columns = rows[0]?.cells.length ?? 0;
    // Rehacer la tabla solo si cambia su forma: en ejecución continua se repinta cada
    // fotograma y basta con cambiar textos y marcas.
    if (grid.dataset.columns !== String(columns) || grid.tBodies[0]?.rows.length !== rows.length) {
      grid.replaceChildren(this.#header(columns), this.#body(rows.length, columns));
      grid.dataset.columns = String(columns);
    }
    const body = grid.tBodies[0];
    if (body === undefined) {
      return;
    }
    rows.forEach((row, r) => {
      const tr = body.rows[r];
      if (tr === undefined) {
        return;
      }
      const address = tr.cells[0];
      if (address !== undefined) {
        address.textContent = row.addressText;
      }
      row.cells.forEach((cell, c) => {
        const td = tr.cells[c + 1];
        if (td === undefined) {
          return;
        }
        td.textContent = cell.text;
        td.title = `0x${cell.address.toString(16).padStart(8, '0')}`;
        setFlag(td, 'access', cell.access);
        setFlag(td, 'target', cell.target ? 'true' : null);
        setFlag(td, 'sp', cell.sp ? 'true' : null);
      });
      const ascii = tr.cells[columns + 1];
      if (ascii !== undefined) {
        ascii.textContent = row.ascii;
      }
    });
  }

  // La tabla tiene alto fijo con scroll propio: se mueve dentro de ella (no la página) hasta la
  // fila de lo que se busca, o de `sp` si se le sigue.
  #reveal(rows: ReturnType<typeof memoryRows>): void {
    const wanted = rows.findIndex((row) =>
      row.cells.some((cell) => (this.#target !== null ? cell.target : this.#followSp && cell.sp)),
    );
    const scroller = this.#ui.grid.parentElement;
    const tr = this.#ui.grid.tBodies[0]?.rows[wanted];
    if (wanted < 0 || scroller === null || tr === undefined) {
      return;
    }
    const head = this.#ui.grid.tHead?.offsetHeight ?? 0;
    const top = tr.offsetTop - head;
    const bottom = tr.offsetTop + tr.offsetHeight - scroller.clientHeight;
    if (scroller.scrollTop > top) {
      scroller.scrollTop = top;
    } else if (scroller.scrollTop < bottom) {
      scroller.scrollTop = bottom;
    }
  }

  #header(columns: number): HTMLTableSectionElement {
    const head = document.createElement('thead');
    const tr = document.createElement('tr');
    const size = ROW_BYTES / columns;
    const titles = [
      'Dirección',
      ...Array.from({ length: columns }, (_, i) => `+${(i * size).toString(16)}`),
      'ASCII',
    ];
    for (const title of titles) {
      const th = document.createElement('th');
      th.scope = 'col';
      th.textContent = title;
      tr.append(th);
    }
    head.append(tr);
    return head;
  }

  #body(rows: number, columns: number): HTMLTableSectionElement {
    const body = document.createElement('tbody');
    for (let r = 0; r < rows; r += 1) {
      const tr = document.createElement('tr');
      const address = document.createElement('th');
      address.scope = 'row';
      address.className = 'memory-address';
      tr.append(address);
      for (let c = 0; c < columns; c += 1) {
        const td = document.createElement('td');
        td.className = 'memory-cell';
        tr.append(td);
      }
      const ascii = document.createElement('td');
      ascii.className = 'memory-ascii';
      tr.append(ascii);
      body.append(tr);
    }
    return body;
  }
}

function setFlag(element: HTMLElement, name: string, value: string | null): void {
  if (value === null) {
    element.removeAttribute(`data-${name}`);
  } else {
    element.dataset[name] = value;
  }
}
