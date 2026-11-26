import {
  RangeSet,
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
  type TransactionSpec,
} from '@codemirror/state';
import { Decoration, EditorView, GutterMarker, gutter, type DecorationSet } from '@codemirror/view';
import type { ExecutionLocation } from '../run/location';

// Lo que el editor sabe de la ejecución: puntos de parada en un margen propio y la línea en
// curso. Los puntos viven en el documento (siguen a su línea al editar); qué dirección paran
// lo decide quien tiene el listado.

class BreakpointMarker extends GutterMarker {
  constructor(readonly inert: boolean) {
    super();
  }

  override eq(other: GutterMarker): boolean {
    return other instanceof BreakpointMarker && other.inert === this.inert;
  }

  override toDOM(): Node {
    const dot = document.createElement('span');
    dot.className = this.inert ? 'cm-breakpoint cm-breakpoint-inert' : 'cm-breakpoint';
    dot.title = this.inert ? 'Punto de parada en una línea sin código' : 'Punto de parada';
    return dot;
  }
}

const activeMarker = new BreakpointMarker(false);
const inertMarker = new BreakpointMarker(true);

// `pos` es el inicio de la línea.
const toggleBreakpointEffect = StateEffect.define<number>({
  map: (pos, mapping) => mapping.mapPos(pos),
});

function hasPointAt(set: RangeSet<GutterMarker>, pos: number): boolean {
  const cursor = set.iter(pos);
  return cursor.value !== null && cursor.from === pos;
}

// Inicios de línea con punto, en orden.
function pointStarts(set: RangeSet<GutterMarker>): number[] {
  const starts: number[] = [];
  const cursor = set.iter();
  while (cursor.value !== null) {
    starts.push(cursor.from);
    cursor.next();
  }
  return starts;
}

const breakpointField = StateField.define<RangeSet<GutterMarker>>({
  create: () => RangeSet.empty,
  update(set, transaction) {
    let next = set.map(transaction.changes);
    for (const effect of transaction.effects) {
      if (effect.is(toggleBreakpointEffect)) {
        next = hasPointAt(next, effect.value)
          ? next.update({ filter: (from) => from !== effect.value })
          : next.update({ add: [activeMarker.range(effect.value)] });
      }
    }
    if (!transaction.docChanged) {
      return next;
    }
    // Al unir o borrar líneas, un punto puede quedar en mitad de una línea o dos en la misma:
    // se lleva cada uno al inicio de su línea y se deja uno por línea.
    const doc = transaction.state.doc;
    const starts = new Set(pointStarts(next).map((from) => doc.lineAt(from).from));
    return RangeSet.of([...starts].sort((a, b) => a - b).map((from) => activeMarker.range(from)));
  },
});

export function breakpointLines(state: EditorState): number[] {
  return pointStarts(state.field(breakpointField)).map((from) => state.doc.lineAt(from).number);
}

// Separado de `toggleBreakpoint` para poder probarlo solo con el estado, sin vista.
export function toggleBreakpointSpec(state: EditorState, line: number): TransactionSpec {
  if (line < 1 || line > state.doc.lines) {
    return {};
  }
  return { effects: toggleBreakpointEffect.of(state.doc.line(line).from) };
}

export function toggleBreakpoint(view: EditorView, line: number): void {
  view.dispatch(toggleBreakpointSpec(view.state, line));
}

export function breakpointsChanged(before: EditorState, after: EditorState): boolean {
  return before.field(breakpointField) !== after.field(breakpointField);
}

// `hasCode` dice si una línea genera instrucciones; un punto en otra se pinta apagado.
function breakpointGutter(hasCode: (state: EditorState, line: number) => boolean): Extension {
  return gutter({
    class: 'cm-breakpoint-gutter',
    markers: (view) => {
      const state = view.state;
      const ranges = pointStarts(state.field(breakpointField)).map((from) => {
        const line = state.doc.lineAt(from).number;
        return (hasCode(state, line) ? activeMarker : inertMarker).range(from);
      });
      return RangeSet.of(ranges);
    },
    initialSpacer: () => activeMarker,
    domEventHandlers: {
      mousedown(view, block) {
        toggleBreakpoint(view, view.state.doc.lineAt(block.from).number);
        return true;
      },
    },
  });
}

const setLocationEffect = StateEffect.define<ExecutionLocation | null>();

export const locationField = StateField.define<ExecutionLocation | null>({
  create: () => null,
  update(value, transaction) {
    for (const effect of transaction.effects) {
      if (effect.is(setLocationEffect)) {
        return effect.value;
      }
    }
    // Tras editar, la línea marcada ya no tiene por qué ser la que se ejecuta.
    return transaction.docChanged ? null : value;
  },
});

const locationDecorations = EditorView.decorations.compute([locationField], (state) => {
  const location = state.field(locationField);
  if (location === null || location.line > state.doc.lines) {
    return Decoration.none;
  }
  const line = state.doc.line(location.line);
  const decoration = Decoration.line({
    class: 'cm-exec-line',
    attributes: { 'data-kind': location.kind },
  });
  return Decoration.set([decoration.range(line.from)]) satisfies DecorationSet;
});

// Marca la línea y la trae a la vista si cambió.
export function showLocation(view: EditorView, location: ExecutionLocation | null): void {
  const current = view.state.field(locationField);
  if (current?.line === location?.line && current?.kind === location?.kind) {
    return;
  }
  const effects: StateEffect<unknown>[] = [setLocationEffect.of(location)];
  if (location !== null && location.line <= view.state.doc.lines) {
    const pos = view.state.doc.line(location.line).from;
    effects.push(EditorView.scrollIntoView(pos, { y: 'nearest', yMargin: 16 }));
  }
  view.dispatch({ effects });
}

export function debugging(hasCode: (state: EditorState, line: number) => boolean): Extension {
  return [breakpointField, breakpointGutter(hasCode), locationField, locationDecorations];
}
