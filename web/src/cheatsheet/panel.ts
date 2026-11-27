import type { CheatSheet, CheatInstruction, PseudoInfo, RegisterInfo } from '../core';
import {
  expansionLines,
  saverLabel,
  sheetSections,
  syscallSignature,
  type SheetSection,
} from './sheet';

export interface CheatSheetElements {
  readonly toggle: HTMLButtonElement;
  readonly drawer: HTMLElement;
  readonly title: HTMLElement;
  readonly close: HTMLButtonElement;
  readonly nav: HTMLElement;
  readonly body: HTMLElement;
}

// Panel lateral con la chuleta. Se superpone a la derecha en vez de ocupar una columna para no
// quitar sitio al editor, y no es modal: se puede seguir escribiendo con ella abierta. Su
// contenido no cambia durante la sesión, así que se construye una sola vez.
export class CheatSheetPanel {
  readonly #elements: CheatSheetElements;
  // Dónde estaba el foco al abrir, para devolverlo al cerrar.
  #returnFocus: HTMLElement | null = null;

  constructor(elements: CheatSheetElements, sheet: CheatSheet) {
    this.#elements = elements;
    const sections = sheetSections(sheet);
    elements.nav.replaceChildren(sectionIndex(sections, elements.body));
    elements.body.replaceChildren(...sections.map(renderSection));
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

function sectionIndex(sections: readonly SheetSection[], body: HTMLElement): HTMLElement {
  const list = document.createElement('ul');
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
  }
  return list;
}

function renderSection(section: SheetSection): HTMLElement {
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
  switch (section.kind) {
    case 'instructions':
      element.append(entryList(section.entries.map(instructionEntry)));
      break;
    case 'pseudo':
      element.append(entryList(section.entries.map(pseudoEntry)));
      break;
    case 'directives':
      element.append(
        entryList(
          section.entries.map((directive) =>
            entry(syntaxCode(directive.name, directive.syntax), null, [text(directive.summary)]),
          ),
        ),
      );
      break;
    case 'syscalls':
      element.append(
        entryList(
          section.entries.map((info) =>
            entry(code(syscallSignature(info), 'cs-syntax'), null, [
              text(`${capitalize(info.name)}. ${info.description}`),
            ]),
          ),
        ),
      );
      break;
    case 'registers':
      element.append(registerTable(section.entries));
      break;
  }
  return element;
}

function instructionEntry(instruction: CheatInstruction): HTMLElement {
  const tag = document.createElement('span');
  tag.className = 'cs-tag';
  const hidden = document.createElement('span');
  hidden.className = 'visually-hidden';
  hidden.textContent = 'formato ';
  tag.append(hidden, instruction.format);
  return entry(syntaxCode(instruction.name, instruction.syntax), tag, [
    text(instruction.summary),
    code(instruction.operation, 'cs-operation'),
  ]);
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
  return entry(syntaxCode(pseudo.name, pseudo.syntax), null, [text(pseudo.summary), expansion]);
}

function registerTable(registers: readonly RegisterInfo[]): HTMLElement {
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
  for (const register of registers) {
    const row = body.insertRow();
    const name = document.createElement('th');
    name.scope = 'row';
    name.textContent = register.name;
    row.append(name);
    row.insertCell().textContent = register.abi;
    row.insertCell().textContent = register.role;
    row.insertCell().textContent = saverLabel(register.saver);
    row.dataset.saver = register.saver;
  }
  return table;
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
