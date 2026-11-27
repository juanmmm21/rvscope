import type { ModelInfo, PipelineTable, RegisterInfo } from '../core';
import { describeTable, tableView, type TableCellView, type TableView } from './table';

export interface PipelineTableElements {
  readonly section: HTMLElement;
  readonly scroller: HTMLElement;
  readonly table: HTMLTableElement;
  readonly caption: HTMLElement;
}

// Margen para dar por hecho que la tabla está desplazada hasta el final: el redondeo de
// `scrollLeft` con zoom deja a veces medio píxel.
const END_TOLERANCE = 4;

// Panel de la tabla del pipeline (instrucción × ciclo). Solo tiene sentido en los modelos
// segmentados: en el de un ciclo cada instrucción ocupa una sola celda y la tabla sería una
// diagonal sin nada que contar, así que el panel se oculta.
export class PipelineTablePanel {
  readonly #elements: PipelineTableElements;
  readonly #registers: readonly RegisterInfo[];

  constructor(elements: PipelineTableElements, registers: readonly RegisterInfo[]) {
    this.#elements = elements;
    this.#registers = registers;
  }

  // `table` es `null` sin programa ensamblado.
  render(model: ModelInfo, table: PipelineTable | null): void {
    const { section, scroller, caption } = this.#elements;
    section.hidden = !model.pipelined;
    if (!model.pipelined) {
      return;
    }
    const view = tableView(
      table ?? { first_cycle: 0, last_cycle: 0, rows: [] },
      this.#registers,
      model.lanes,
    );
    // Se sigue al ciclo en curso (a la derecha) y a la instrucción más nueva (abajo), salvo
    // que se haya desplazado hacia atrás para mirar algo.
    const followRight =
      scroller.scrollLeft + scroller.clientWidth >= scroller.scrollWidth - END_TOLERANCE;
    const followBottom =
      scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - END_TOLERANCE;
    this.#fill(view);
    caption.textContent = describeTable(view);
    if (followRight) {
      scroller.scrollLeft = scroller.scrollWidth;
    }
    if (followBottom) {
      scroller.scrollTop = scroller.scrollHeight;
    }
  }

  #fill(view: TableView): void {
    const table = this.#elements.table;
    const head = document.createElement('thead');
    const headRow = head.insertRow();
    const corner = document.createElement('th');
    corner.scope = 'col';
    corner.className = 'pt-instruction';
    corner.textContent = 'Instrucción';
    headRow.append(corner);
    for (const cycle of view.cycles) {
      const th = document.createElement('th');
      th.scope = 'col';
      th.textContent = String(cycle);
      if (cycle === view.current) {
        th.dataset.current = 'true';
        th.setAttribute('aria-current', 'step');
      }
      headRow.append(th);
    }
    const body = document.createElement('tbody');
    for (const row of view.rows) {
      const tr = body.insertRow();
      const th = document.createElement('th');
      th.scope = 'row';
      th.className = 'pt-instruction';
      const text = document.createElement('span');
      text.className = 'pt-text';
      text.textContent = row.label;
      const address = document.createElement('span');
      address.className = 'pt-address';
      address.textContent =
        row.line === null ? row.address : `${row.address} · l. ${String(row.line)}`;
      th.append(text, address);
      tr.append(th);
      for (const cell of row.cells) {
        tr.append(cellElement(cell));
      }
    }
    const caption = document.createElement('caption');
    caption.className = 'visually-hidden';
    caption.textContent = 'Etapa de cada instrucción en cada ciclo';
    table.replaceChildren(caption, head, body);
  }
}

function cellElement(cell: TableCellView | null): HTMLTableCellElement {
  const td = document.createElement('td');
  if (cell === null) {
    return td;
  }
  td.dataset.state = cell.state;
  td.dataset.trap = String(cell.trap);
  if (cell.current) {
    td.dataset.current = 'true';
  }
  td.title = cell.description;
  // Lo que se ve es abreviado; el lector de pantalla lee la frase entera.
  const visible = document.createElement('span');
  visible.setAttribute('aria-hidden', 'true');
  const stage = document.createElement('span');
  stage.className = 'pt-stage';
  stage.textContent = cell.stage;
  visible.append(stage);
  if (cell.lane !== null) {
    const lane = document.createElement('sub');
    lane.className = 'pt-lane';
    lane.textContent = String(cell.lane);
    visible.append(lane);
  }
  if (cell.bypasses.length > 0) {
    const marks = document.createElement('span');
    marks.className = 'pt-bypass';
    marks.textContent = cell.bypasses.join(' ');
    visible.append(marks);
  }
  const spoken = document.createElement('span');
  spoken.className = 'visually-hidden';
  spoken.textContent = cell.description;
  td.append(visible, spoken);
  return td;
}
