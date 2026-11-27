import type { CheatSheet, CheatInstruction, PseudoInfo, RegisterInfo } from '../core';
import {
  directiveSearchText,
  expansionLines,
  fieldCells,
  foldText,
  hexWord,
  instructionSearchText,
  matchesTerms,
  pseudoSearchText,
  queryTerms,
  registerSearchText,
  saverLabel,
  searchSummary,
  sheetSections,
  syscallSearchText,
  syscallSignature,
  type SheetSection,
} from './sheet';

export interface CheatSheetElements {
  readonly toggle: HTMLButtonElement;
  readonly drawer: HTMLElement;
  readonly title: HTMLElement;
  readonly close: HTMLButtonElement;
  readonly nav: HTMLElement;
  readonly search: HTMLInputElement;
  readonly searchStatus: HTMLElement;
  readonly body: HTMLElement;
}

// Una fila que el buscador puede esconder, con su texto ya plegado.
interface SearchRow {
  readonly element: HTMLElement;
  readonly text: string;
}

// Una sección con sus filas y su enlace del índice, que se esconden si no queda ninguna.
interface SearchSection {
  readonly element: HTMLElement;
  readonly link: HTMLElement;
  readonly rows: readonly SearchRow[];
}

// Panel lateral con la chuleta. Se superpone a la derecha en vez de ocupar una columna para no
// quitar sitio al editor, y no es modal: se puede seguir escribiendo con ella abierta. Su
// contenido no cambia durante la sesión, así que se construye una sola vez.
export class CheatSheetPanel {
  readonly #elements: CheatSheetElements;
  // Dónde estaba el foco al abrir, para devolverlo al cerrar.
  #returnFocus: HTMLElement | null = null;
  readonly #sections: readonly SearchSection[];

  constructor(elements: CheatSheetElements, sheet: CheatSheet) {
    this.#elements = elements;
    const sections = sheetSections(sheet);
    const index = sectionIndex(sections, elements.body);
    elements.nav.replaceChildren(index.list);
    const rendered = sections.map(renderSection);
    elements.body.replaceChildren(...rendered.map((section) => section.element));
    this.#sections = rendered.map((section, position) => {
      const link = index.items[position];
      if (link === undefined) {
        throw new Error('el índice de la chuleta no cuadra con sus secciones');
      }
      return { ...section, link };
    });
    elements.search.addEventListener('input', () => {
      this.#filter();
    });
    elements.search.addEventListener('keydown', (event) => {
      // Escape con texto solo borra la búsqueda; con el buscador vacío cierra el panel.
      if (event.key === 'Escape' && elements.search.value !== '') {
        event.preventDefault();
        event.stopPropagation();
        this.search('');
      } else if (event.key === 'Enter') {
        event.preventDefault();
        this.#focusFirstResult();
      }
    });
    elements.toggle.addEventListener('click', () => {
      this.toggle();
    });
    elements.close.addEventListener('click', () => {
      this.close();
    });
    elements.drawer.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        this.close();
      }
    });
  }

  // Filtra la chuleta como si se hubiera escrito `query` en el buscador.
  search(query: string): void {
    this.#elements.search.value = query;
    this.#filter();
  }

  #filter(): void {
    const query = this.#elements.search.value;
    const terms = queryTerms(query);
    let matches = 0;
    for (const section of this.#sections) {
      let visible = 0;
      for (const row of section.rows) {
        const show = matchesTerms(row.text, terms);
        row.element.hidden = !show;
        if (show) {
          visible += 1;
        }
      }
      section.element.hidden = visible === 0;
      section.link.hidden = visible === 0;
      matches += visible;
    }
    this.#elements.searchStatus.textContent = searchSummary(query, matches);
    this.#elements.body.scrollTop = 0;
  }

  // Enter en el buscador lleva a la primera instrucción que quede (su botón abre la ficha);
  // si solo quedan filas sin botón, al cuerpo, que se desplaza con el teclado.
  #focusFirstResult(): void {
    const { body } = this.#elements;
    const first = body.querySelector<HTMLElement>(
      '.cs-section:not([hidden]) .cs-entry:not([hidden]) .cs-open',
    );
    (first ?? body).focus();
  }

  // Abre la chuleta en la entrada de `name` (instrucción, pseudoinstrucción o directiva): quita
  // la búsqueda, despliega la ficha si es una instrucción y deja el foco en ella. La instrucción
  // real va antes que la pseudoinstrucción del mismo nombre.
  reveal(name: string): void {
    this.open();
    this.search('');
    const { body } = this.#elements;
    const row = body.querySelector<HTMLElement>(`.cs-entry[data-name="${CSS.escape(name)}"]`);
    if (row === null) {
      return;
    }
    const button = row.querySelector<HTMLButtonElement>('.cs-open');
    if (button !== null && button.getAttribute('aria-expanded') !== 'true') {
      button.click();
    }
    const target = button ?? row;
    if (button === null) {
      row.tabIndex = -1;
    }
    target.scrollIntoView({ block: 'start' });
    target.focus({ preventScroll: true });
  }

  get isOpen(): boolean {
    return !this.#elements.drawer.hidden;
  }

  toggle(): void {
    if (this.isOpen) {
      this.close();
    } else {
      this.open();
    }
  }

  open(): void {
    const { drawer, toggle, title } = this.#elements;
    if (this.isOpen) {
      title.focus();
      return;
    }
    const active = document.activeElement;
    this.#returnFocus = active instanceof HTMLElement && active !== document.body ? active : null;
    drawer.hidden = false;
    toggle.setAttribute('aria-expanded', 'true');
    title.focus();
  }

  close(): void {
    const { drawer, toggle } = this.#elements;
    if (!this.isOpen) {
      return;
    }
    const focusWasInside = drawer.contains(document.activeElement);
    drawer.hidden = true;
    toggle.setAttribute('aria-expanded', 'false');
    // Solo se mueve el foco si estaba dentro: si se cerró con el atajo desde el editor, el
    // foco se queda donde está.
    if (focusWasInside) {
      const target =
        this.#returnFocus !== null && this.#returnFocus.isConnected ? this.#returnFocus : toggle;
      target.focus();
    }
    this.#returnFocus = null;
  }
}

function sectionIndex(
  sections: readonly SheetSection[],
  body: HTMLElement,
): { list: HTMLElement; items: HTMLElement[] } {
  const list = document.createElement('ul');
  const items: HTMLElement[] = [];
  for (const section of sections) {
    const item = document.createElement('li');
    const link = document.createElement('a');
    link.href = `#${section.id}`;
    link.textContent = section.title;
    // Sin cambiar la dirección de la página: se desplaza el panel y el foco va al título, para
    // que el siguiente Tab siga desde la sección elegida.
    link.addEventListener('click', (event) => {
      event.preventDefault();
      const heading = body.querySelector<HTMLElement>(`#${section.id}`);
      if (heading !== null) {
        heading.scrollIntoView({ block: 'start' });
        heading.focus({ preventScroll: true });
      }
    });
    item.append(link);
    list.append(item);
    items.push(item);
  }
  return { list, items };
}

function renderSection(section: SheetSection): Omit<SearchSection, 'link'> {
  const element = document.createElement('section');
  element.className = 'cs-section';
  element.dataset.kind = section.kind;
  const heading = document.createElement('h3');
  heading.id = section.id;
  heading.className = 'label cs-heading';
  heading.tabIndex = -1;
  heading.textContent = section.title;
  element.setAttribute('aria-labelledby', section.id);
  element.append(heading);
  let rows: SearchRow[];
  switch (section.kind) {
    case 'instructions':
      rows = section.entries.map((instruction) => ({
        element: instructionEntry(instruction),
        text: instructionSearchText(instruction),
      }));
      break;
    case 'pseudo':
      rows = section.entries.map((pseudo) => ({
        element: pseudoEntry(pseudo),
        text: pseudoSearchText(pseudo),
      }));
      break;
    case 'directives':
      rows = section.entries.map((directive) => {
        const row = entry(syntaxCode(directive.name, directive.syntax), null, [
          text(directive.summary),
        ]);
        row.dataset.name = directive.name;
        return { element: row, text: directiveSearchText(directive) };
      });
      break;
    case 'syscalls':
      rows = section.entries.map((info) => ({
        element: entry(code(syscallSignature(info), 'cs-syntax'), null, [
          text(`${capitalize(info.name)}. ${info.description}`),
        ]),
        text: syscallSearchText(info),
      }));
      break;
    case 'registers': {
      const table = registerTable(section.entries);
      element.append(table.element);
      rows = table.rows;
      break;
    }
  }
  if (section.kind !== 'registers') {
    element.append(entryList(rows.map((row) => row.element)));
  }
  // El título del grupo también cuenta: «división» o «cargas» enseñan el grupo entero.
  const title = foldText(section.title);
  return { element, rows: rows.map((row) => ({ ...row, text: `${title}\n${row.text}` })) };
}

// La fila de una instrucción se despliega en su ficha: el término es un botón para que se abra
// con el teclado igual que con el ratón, y la ficha va en un `dd` más de la propia fila.
function instructionEntry(instruction: CheatInstruction): HTMLElement {
  const tag = document.createElement('span');
  tag.className = 'cs-tag';
  const hidden = document.createElement('span');
  hidden.className = 'visually-hidden';
  hidden.textContent = 'formato ';
  tag.append(hidden, instruction.format);

  const card = instructionCard(instruction);
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'cs-open';
  button.setAttribute('aria-expanded', 'false');
  button.setAttribute('aria-controls', card.id);
  button.append(syntaxCode(instruction.name, instruction.syntax), tag);
  button.addEventListener('click', () => {
    const expand = card.hidden;
    card.hidden = !expand;
    button.setAttribute('aria-expanded', String(expand));
  });

  const row = entry(button, null, [
    text(instruction.summary),
    code(instruction.operation, 'cs-operation'),
  ]);
  row.dataset.instruction = instruction.name;
  row.dataset.name = instruction.name;
  row.append(card);
  return row;
}

function instructionCard(instruction: CheatInstruction): HTMLElement {
  const card = document.createElement('dd');
  card.id = `cs-card-${instruction.name.replace('.', '-')}`;
  card.className = 'cs-card';
  card.hidden = true;

  const example = document.createElement('p');
  example.className = 'cs-example';
  const label = document.createElement('span');
  label.className = 'label';
  label.textContent = 'Ejemplo';
  example.append(label, ' ', syntaxCode(instruction.name, instruction.example));
  if (instruction.example_word !== null) {
    example.append(' ', code(`→ ${hexWord(instruction.example_word)}`, 'cs-operation cs-word'));
  }

  const extension = instruction.extension === 'M' ? 'extensión M' : 'base RV32I';
  const note = document.createElement('p');
  note.className = 'cs-note';
  note.textContent = `Formato ${instruction.format} · ${extension}. Sombreados, los campos fijos que identifican la instrucción; el resto lo pone el programa.`;

  card.append(example, fieldTable(instruction), note);
  return card;
}

// Una columna por campo, con el ancho proporcional a sus bits como en los diagramas de clase,
// y en las filas el tramo de bits y lo que vale cada campo en el ejemplo. Sin desplazamiento
// horizontal: en un móvil los nombres largos se parten antes que esconder la mitad de la palabra.
function fieldTable(instruction: CheatInstruction): HTMLElement {
  const cells = fieldCells(instruction);
  const table = document.createElement('table');
  table.className = 'cs-fields';
  const caption = document.createElement('caption');
  caption.className = 'visually-hidden';
  caption.textContent = `Campos de bits de ${instruction.name}, con su valor en el ejemplo`;
  table.append(caption);
  const columns = document.createElement('colgroup');
  for (const cell of cells) {
    const column = document.createElement('col');
    column.style.width = `${String((cell.width / 32) * 100)}%`;
    columns.append(column);
  }
  table.append(columns);

  const head = table.createTHead().insertRow();
  for (const cell of cells) {
    const th = document.createElement('th');
    th.scope = 'col';
    th.append(...breakableName(cell.name));
    if (cell.fixed) {
      th.className = 'cs-fixed';
      const fixed = document.createElement('span');
      fixed.className = 'visually-hidden';
      fixed.textContent = ' (fijo)';
      th.append(fixed);
    }
    head.append(th);
  }
  const body = table.createTBody();
  const ranges = body.insertRow();
  ranges.className = 'cs-range';
  const bits = body.insertRow();
  bits.className = 'cs-bits';
  for (const cell of cells) {
    const range = ranges.insertCell();
    range.textContent = cell.range;
    const value = bits.insertCell();
    value.textContent = cell.bits ?? '·'.repeat(cell.width);
    if (cell.fixed) {
      range.className = 'cs-fixed';
      value.className = 'cs-fixed';
    }
  }
  return table;
}

function pseudoEntry(pseudo: PseudoInfo): HTMLElement {
  const expansion = document.createElement('span');
  expansion.className = 'cs-expansion';
  const lines = expansionLines(pseudo);
  lines.forEach((line, index) => {
    if (index > 0) {
      expansion.append(document.createElement('br'), 'o bien ');
    }
    expansion.append(code(`→ ${line}`, 'cs-operation'));
  });
  const row = entry(syntaxCode(pseudo.name, pseudo.syntax), null, [
    text(pseudo.summary),
    expansion,
  ]);
  row.dataset.name = pseudo.name;
  return row;
}

function registerTable(registers: readonly RegisterInfo[]): {
  element: HTMLElement;
  rows: SearchRow[];
} {
  const table = document.createElement('table');
  table.className = 'cs-registers';
  const head = table.createTHead().insertRow();
  for (const title of ['Registro', 'ABI', 'Uso', 'En una llamada']) {
    const cell = document.createElement('th');
    cell.scope = 'col';
    cell.className = 'label';
    cell.textContent = title;
    head.append(cell);
  }
  const body = table.createTBody();
  const rows: SearchRow[] = [];
  for (const register of registers) {
    const row = body.insertRow();
    rows.push({ element: row, text: registerSearchText(register) });
    const name = document.createElement('th');
    name.scope = 'row';
    name.textContent = register.name;
    row.append(name);
    row.insertCell().textContent = register.abi;
    row.insertCell().textContent = register.role;
    row.insertCell().textContent = saverLabel(register.saver);
    row.dataset.saver = register.saver;
  }
  return { element: table, rows };
}

// Las entradas van en una lista de definiciones: el término es la sintaxis y la descripción lo
// que hace; un `div` por entrada para poder darle forma de fila.
function entryList(entries: readonly HTMLElement[]): HTMLElement {
  const list = document.createElement('dl');
  list.className = 'cs-list';
  list.append(...entries);
  return list;
}

function entry(term: HTMLElement, tag: HTMLElement | null, details: readonly Node[]): HTMLElement {
  const row = document.createElement('div');
  row.className = 'cs-entry';
  const dt = document.createElement('dt');
  dt.append(term);
  if (tag !== null) {
    dt.append(tag);
  }
  row.append(dt);
  for (const detail of details) {
    const dd = document.createElement('dd');
    dd.append(detail);
    row.append(dd);
  }
  return row;
}

// Sintaxis con el mnemónico destacado: «addi» y luego los operandos.
function syntaxCode(name: string, syntax: string): HTMLElement {
  const element = code('', 'cs-syntax');
  const mnemonic = document.createElement('span');
  mnemonic.className = 'cs-mnemonic';
  mnemonic.textContent = name;
  element.append(mnemonic);
  const rest = syntax.startsWith(name) ? syntax.slice(name.length) : ` ${syntax}`;
  if (rest !== '') {
    element.append(rest);
  }
  return element;
}

// Los nombres de los inmediatos partidos (`imm[12|10:5]`) no caben en una columna estrecha: se
// deja partirlos tras «|» en vez de en cualquier letra.
function breakableName(name: string): Node[] {
  return name
    .split('|')
    .flatMap((part, index) =>
      index === 0 ? [text(part)] : [text('|'), document.createElement('wbr'), text(part)],
    );
}

function code(content: string, className: string): HTMLElement {
  const element = document.createElement('code');
  element.className = className;
  element.textContent = content;
  return element;
}

function text(content: string): Text {
  return document.createTextNode(content);
}

function capitalize(content: string): string {
  return content.charAt(0).toUpperCase() + content.slice(1);
}
