import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { lintGutter, lintKeymap, setDiagnostics } from '@codemirror/lint';
import { EditorState, StateEffect, StateField, type Extension } from '@codemirror/state';
import {
  EditorView,
  ViewPlugin,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  type PluginValue,
  type ViewUpdate,
} from '@codemirror/view';
import { tags } from '@lezer/highlight';
import type { AssembleReport, Vocabulary } from '../core';
import { toDiagnostics } from './diagnostics';
import { listingRows, type ListingRow } from './listing';
import { riscvLanguage } from './riscv';

export interface EditorOptions {
  readonly parent: HTMLElement;
  // Columna del desensamblado, a la derecha del editor y del mismo alto.
  readonly listingParent: HTMLElement;
  readonly vocabulary: Vocabulary;
  readonly source: string;
  readonly onChange: (source: string) => void;
}

export interface SourceEditor {
  readonly view: EditorView;
  source(): string;
  showReport(report: AssembleReport): void;
}

interface ListingState {
  readonly rows: ReadonlyMap<number, ListingRow>;
  // El texto cambió desde el último ensamblado: las filas pueden estar descolocadas.
  readonly stale: boolean;
}

const setListing = StateEffect.define<ReadonlyMap<number, ListingRow>>();

const listingField = StateField.define<ListingState>({
  create: () => ({ rows: new Map(), stale: false }),
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setListing)) {
        return { rows: effect.value, stale: false };
      }
    }
    return transaction.docChanged && !value.stale ? { ...value, stale: true } : value;
  },
});

// Las filas se colocan con la altura que CodeMirror calcula para cada línea y la columna se
// desplaza con el editor: así el desensamblado queda a la altura de su línea aunque una línea
// cambie de alto, sin meter widgets dentro del texto.
function listingColumn(pane: HTMLElement): Extension {
  return ViewPlugin.fromClass(
    class implements PluginValue {
      readonly #view: EditorView;
      readonly #track: HTMLElement;
      readonly #onScroll = (): void => {
        this.#follow();
      };

      constructor(view: EditorView) {
        this.#view = view;
        this.#track = document.createElement('div');
        this.#track.className = 'listing-track';
        pane.replaceChildren(this.#track);
        view.scrollDOM.addEventListener('scroll', this.#onScroll, { passive: true });
        this.#render();
      }

      update(update: ViewUpdate): void {
        const listingChanged =
          update.startState.field(listingField) !== update.state.field(listingField);
        if (
          listingChanged ||
          update.docChanged ||
          update.viewportChanged ||
          update.geometryChanged
        ) {
          this.#render();
        }
      }

      destroy(): void {
        this.#view.scrollDOM.removeEventListener('scroll', this.#onScroll);
        this.#track.remove();
      }

      #follow(): void {
        this.#track.style.transform = `translateY(${String(-this.#view.scrollDOM.scrollTop)}px)`;
      }

      #render(): void {
        const view = this.#view;
        const { rows, stale } = view.state.field(listingField);
        const offset = view.documentPadding.top;
        pane.dataset.stale = String(stale);
        this.#track.style.height = `${String(view.contentHeight)}px`;
        const children: HTMLElement[] = [];
        for (const block of view.viewportLineBlocks) {
          const row = rows.get(view.state.doc.lineAt(block.from).number);
          if (row === undefined) {
            continue;
          }
          const element = document.createElement('div');
          element.className = 'listing-row';
          element.style.top = `${String(block.top + offset)}px`;
          element.style.height = `${String(block.height)}px`;
          element.title = row.detail;
          const cells: [string, string][] = [
            ['listing-address', row.address],
            ['listing-code', row.code],
            ['listing-text', row.text],
          ];
          element.append(
            ...cells.map(([className, text]) => {
              const cell = document.createElement('span');
              cell.className = className;
              cell.textContent = text;
              return cell;
            }),
          );
          children.push(element);
        }
        this.#track.replaceChildren(...children);
        this.#follow();
      }
    },
  );
}

// Colores por clase: los define styles.css para cada tema, así el modo claro y el oscuro
// salen de las mismas variables que el resto de la interfaz.
const highlightStyle = HighlightStyle.define([
  { tag: tags.keyword, class: 'tok-instruction' },
  { tag: tags.macroName, class: 'tok-pseudo' },
  { tag: tags.processingInstruction, class: 'tok-directive' },
  { tag: tags.special(tags.variableName), class: 'tok-register' },
  { tag: tags.definition(tags.labelName), class: 'tok-label-def' },
  { tag: tags.labelName, class: 'tok-label' },
  { tag: tags.modifier, class: 'tok-modifier' },
  { tag: tags.number, class: 'tok-number' },
  { tag: [tags.string, tags.character], class: 'tok-string' },
  { tag: tags.lineComment, class: 'tok-comment' },
  { tag: [tags.operator, tags.punctuation], class: 'tok-punctuation' },
  { tag: tags.invalid, class: 'tok-invalid' },
]);

const theme = EditorView.theme({
  '&': { height: '100%', color: 'var(--fg)', backgroundColor: 'transparent' },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': { fontFamily: 'var(--font-mono)', lineHeight: 'var(--code-line)' },
  '.cm-content': { caretColor: 'var(--fg)' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--fg)' },
  '.cm-gutters': {
    backgroundColor: 'var(--bg)',
    color: 'var(--muted)',
    border: 'none',
    borderRight: '1px solid var(--line)',
  },
  '.cm-activeLine, .cm-activeLineGutter': { backgroundColor: 'var(--surface)' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground':
    { backgroundColor: 'var(--selection)' },
  '.cm-tooltip': {
    backgroundColor: 'var(--bg)',
    color: 'var(--fg)',
    border: '1px solid var(--line)',
  },
  '.cm-diagnostic': { fontFamily: 'var(--font-mono)', fontSize: '0.8rem' },
  '.cm-diagnostic-error': { borderLeftColor: 'var(--danger)' },
  '.cm-panels': { backgroundColor: 'var(--bg)', color: 'var(--fg)' },
});

export function createEditor(options: EditorOptions): SourceEditor {
  const view = new EditorView({
    parent: options.parent,
    state: EditorState.create({
      doc: options.source,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        drawSelection(),
        history(),
        lintGutter(),
        listingField,
        listingColumn(options.listingParent),
        riscvLanguage(options.vocabulary),
        syntaxHighlighting(highlightStyle),
        theme,
        EditorView.contentAttributes.of({ 'aria-label': 'Código en ensamblador' }),
        // Tab indenta; Escape y luego Tab saca el foco del editor (convenio de CodeMirror).
        keymap.of([...defaultKeymap, ...historyKeymap, ...lintKeymap, indentWithTab]),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            options.onChange(update.state.doc.toString());
          }
        }),
      ],
    }),
  });

  return {
    view,
    source: () => view.state.doc.toString(),
    showReport(report: AssembleReport): void {
      const diagnostics = setDiagnostics(view.state, toDiagnostics(view.state.doc, report.errors));
      const rows = report.ok ? listingRows(report.listing) : new Map<number, ListingRow>();
      view.dispatch(diagnostics, { effects: setListing.of(rows) });
    },
  };
}
