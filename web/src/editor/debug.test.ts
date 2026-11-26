import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { breakpointLines, debugging, toggleBreakpointSpec } from './debug';

function editorState(doc: string): EditorState {
  return EditorState.create({ doc, extensions: debugging(() => true) });
}

function toggle(state: EditorState, ...lines: number[]): EditorState {
  return lines.reduce(
    (current, line) => current.update(toggleBreakpointSpec(current, line)).state,
    state,
  );
}

describe('puntos de parada', () => {
  it('se ponen y se quitan por línea', () => {
    const state = toggle(editorState('a\nb\nc\nd'), 3, 1, 3);
    expect(breakpointLines(state)).toEqual([1]);
  });

  it('ignoran líneas fuera del documento', () => {
    expect(breakpointLines(toggle(editorState('a\nb'), 0, 7))).toEqual([]);
  });

  it('siguen a su línea al insertar texto encima', () => {
    const state = toggle(editorState('a\nb\nc'), 3);
    const moved = state.update({ changes: { from: 0, insert: 'x\ny\n' } }).state;
    expect(breakpointLines(moved)).toEqual([5]);
  });

  it('al unir dos líneas con punto queda uno al inicio', () => {
    const state = toggle(editorState('ab\ncd\nef'), 1, 2);
    // Borra el salto de línea entre «ab» y «cd».
    const joined = state.update({ changes: { from: 2, to: 3 } }).state;
    expect(breakpointLines(joined)).toEqual([1]);
    expect(breakpointLines(toggle(joined, 1))).toEqual([]);
  });
});
