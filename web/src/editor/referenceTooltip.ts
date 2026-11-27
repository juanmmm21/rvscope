import { Prec, StateEffect, StateField, type EditorState, type Extension } from '@codemirror/state';
import { EditorView, hoverTooltip, keymap, showTooltip, type Tooltip } from '@codemirror/view';
import {
  instructionFootnote,
  lookupReference,
  operationWordAt,
  type OperationWord,
  type ReferenceEntry,
  type ReferenceIndex,
} from './reference';

// Ficha de la chuleta dentro del editor. Sale al pasar el ratón por una operación y también con
// el cursor de texto: al moverlo con el teclado hasta una operación aparece encima de ella, y se
// quita al escribir (para no tapar lo que se teclea), con Escape o al salir del editor.

interface Located {
  readonly word: OperationWord;
  readonly from: number;
  readonly to: number;
  readonly entries: readonly ReferenceEntry[];
}

function locate(state: EditorState, index: ReferenceIndex, pos: number): Located | null {
  const line = state.doc.lineAt(pos);
  const word = operationWordAt(line.text, pos - line.from);
  if (word === null) {
    return null;
  }
  const entries = lookupReference(index, word.word);
  if (entries.length === 0) {
    return null;
  }
  return { word, from: line.from + word.from, to: line.from + word.to, entries };
}

// Operación bajo el cursor (selección vacía), para abrir la chuleta en su ficha.
export function operationAtCursor(state: EditorState, index: ReferenceIndex): string | null {
  const { main } = state.selection;
  if (!main.empty) {
    return null;
  }
  return locate(state, index, main.head)?.word.word.toLowerCase() ?? null;
}

function tooltipFor(located: Located): Tooltip {
  return {
    pos: located.from,
    end: located.to,
    above: true,
    create: () => ({ dom: referenceCard(located.entries) }),
  };
}

const hideCard = StateEffect.define();

function cursorCard(index: ReferenceIndex): StateField<Tooltip | null> {
  return StateField.define<Tooltip | null>({
    create: () => null,
    update(value, transaction) {
      if (transaction.effects.some((effect) => effect.is(hideCard)) || transaction.docChanged) {
        return null;
      }
      if (transaction.selection === undefined) {
        return value;
      }
      // Solo los movimientos con el teclado: con el ratón ya está la ficha del hover, y dos
      // fichas iguales a la vez estorban.
      if (!transaction.isUserEvent('select') || transaction.isUserEvent('select.pointer')) {
        return null;
      }
      const { main } = transaction.state.selection;
      if (!main.empty) {
        return null;
      }
      const located = locate(transaction.state, index, main.head);
      if (located === null) {
        return null;
      }
      // La misma palabra: se conserva la ficha para que no parpadee al moverse dentro de ella.
      if (value !== null && value.pos === located.from && value.end === located.to) {
        return value;
      }
      return tooltipFor(located);
    },
    provide: (field) => showTooltip.from(field),
  });
}

export function referenceTooltips(index: ReferenceIndex): Extension {
  const field = cursorCard(index);
  return [
    field,
    hoverTooltip((view, pos) => {
      const located = locate(view.state, index, pos);
      return located === null ? null : tooltipFor(located);
    }),
    EditorView.focusChangeEffect.of((_state, focusing) => (focusing ? null : hideCard.of(null))),
    // Por delante del mapa por defecto, que usa Escape para quedarse con una sola selección.
    Prec.high(
      keymap.of([
        {
          key: 'Escape',
          // Solo se come la tecla si había ficha: si no, Escape sigue sirviendo para salir del
          // editor con Tab.
          run: (view) => {
            if (view.state.field(field) === null) {
              return false;
            }
            view.dispatch({ effects: hideCard.of(null) });
            return true;
          },
        },
      ]),
    ),
  ];
}

function referenceCard(entries: readonly ReferenceEntry[]): HTMLElement {
  const card = document.createElement('div');
  card.className = 'rf-card';
  card.setAttribute('role', 'tooltip');
  for (const entry of entries) {
    card.append(referenceEntry(entry));
  }
  const hint = document.createElement('p');
  hint.className = 'rf-hint';
  hint.textContent = 'F2 la abre en la chuleta';
  card.append(hint);
  return card;
}

function referenceEntry(entry: ReferenceEntry): HTMLElement {
  const element = document.createElement('div');
  element.className = 'rf-entry';
  element.dataset.kind = entry.kind;
  const head = document.createElement('p');
  head.className = 'rf-head';
  head.append(syntaxCode(entry.info.name, entry.info.syntax));
  const tag = document.createElement('span');
  tag.className = 'rf-tag';
  const summary = paragraph('rf-summary', entry.info.summary);
  switch (entry.kind) {
    case 'instruction':
      tag.textContent = `formato ${entry.info.format}`;
      head.append(tag);
      element.append(
        head,
        summary,
        paragraph('rf-operation', entry.info.operation),
        paragraph('rf-note', instructionFootnote(entry.info)),
      );
      break;
    case 'pseudo':
      tag.textContent = 'pseudoinstrucción';
      head.append(tag);
      element.append(head, summary, paragraph('rf-operation', `→ ${entry.info.expansion}`));
      break;
    case 'directive':
      tag.textContent = 'directiva';
      head.append(tag);
      element.append(head, summary);
      break;
  }
  return element;
}

function syntaxCode(name: string, syntax: string): HTMLElement {
  const element = document.createElement('code');
  element.className = 'rf-syntax';
  const mnemonic = document.createElement('span');
  mnemonic.className = 'rf-mnemonic';
  mnemonic.textContent = name;
  element.append(mnemonic);
  const rest = syntax.startsWith(name) ? syntax.slice(name.length) : ` ${syntax}`;
  if (rest !== '') {
    element.append(rest);
  }
  return element;
}

function paragraph(className: string, content: string): HTMLElement {
  const element = document.createElement('p');
  element.className = className;
  element.textContent = content;
  return element;
}
