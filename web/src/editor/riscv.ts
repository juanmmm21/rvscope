import { StreamLanguage, type StreamParser, type StringStream } from '@codemirror/language';
import { tags, type Tag } from '@lezer/highlight';
import type { Vocabulary } from '../core';

// Resaltado de RISC-V. Sigue las reglas del lexer de `rv-asm` (comentario tras `#`, `;` separa
// sentencias, etiquetas `nombre:` y `1:`, referencias `1f`/`1b`, `%hi(...)`) y saca las palabras
// del vocabulario que exporta el wasm: el editor no tiene listas propias.

export type RiscvToken =
  | 'comment'
  | 'instruction'
  | 'pseudo'
  | 'directive'
  | 'register'
  | 'labelDef'
  | 'labelRef'
  | 'modifier'
  | 'number'
  | 'string'
  | 'character'
  | 'operator'
  | 'punctuation'
  | 'invalid';

interface Words {
  readonly instructions: ReadonlySet<string>;
  readonly pseudo: ReadonlySet<string>;
  readonly directives: ReadonlySet<string>;
  readonly registers: ReadonlySet<string>;
  readonly modifiers: ReadonlySet<string>;
}

export interface RiscvState {
  // Cierto mientras la siguiente palabra de la sentencia es su mnemónico o directiva.
  expectsOperation: boolean;
}

const IDENT = /^[A-Za-z_.$][\w.$]*/;
const NUMBER = /^(?:0[xX][0-9a-fA-F_]+|0[bB][01_]+|\d[\d_]*)/;
const LOCAL_REF = /^\d+[fb](?![\w.$])/;
const LOCAL_DEF = /^\d+:/;

function words(vocabulary: Vocabulary): Words {
  const lower = (list: readonly string[]): ReadonlySet<string> =>
    new Set(list.map((word) => word.toLowerCase()));
  return {
    instructions: lower(vocabulary.instructions),
    pseudo: lower(vocabulary.pseudo_instructions),
    directives: lower(vocabulary.directives),
    registers: lower(vocabulary.registers),
    modifiers: lower(vocabulary.modifiers),
  };
}

function quoted(stream: StringStream, quote: string): void {
  let escaped = false;
  for (let next = stream.next(); next !== undefined; next = stream.next()) {
    if (next === quote && !escaped) {
      return;
    }
    escaped = !escaped && next === '\\';
  }
}

function operation(word: string, known: Words): RiscvToken | null {
  const lower = word.toLowerCase();
  if (known.directives.has(lower)) return 'directive';
  if (known.instructions.has(lower)) return 'instruction';
  if (known.pseudo.has(lower)) return 'pseudo';
  // Un mnemónico desconocido lo señala el ensamblador con su mensaje; aquí basta con no colorearlo.
  return null;
}

export function riscvParser(vocabulary: Vocabulary): StreamParser<RiscvState> {
  const known = words(vocabulary);
  return {
    name: 'riscv',
    startState: () => ({ expectsOperation: true }),
    copyState: (state) => ({ ...state }),
    token(stream, state): RiscvToken | null {
      if (stream.sol()) {
        state.expectsOperation = true;
      }
      if (stream.eatSpace()) {
        return null;
      }
      const start = stream.peek();
      if (start === '#') {
        stream.skipToEnd();
        return 'comment';
      }
      if (start === ';') {
        stream.next();
        state.expectsOperation = true;
        return 'punctuation';
      }
      if (start === '"' || start === "'") {
        stream.next();
        quoted(stream, start);
        return start === '"' ? 'string' : 'character';
      }
      if (start === '%') {
        stream.next();
        const name = stream.match(IDENT, true) as RegExpMatchArray | null;
        return name !== null && known.modifiers.has(name[0].toLowerCase()) ? 'modifier' : 'invalid';
      }
      if (state.expectsOperation && stream.match(LOCAL_DEF, false) !== null) {
        stream.match(/^\d+/);
        return 'labelDef';
      }
      if (stream.match(LOCAL_REF) !== null) {
        return 'labelRef';
      }
      if (stream.match(NUMBER) !== null) {
        return 'number';
      }
      const ident = stream.match(IDENT) as RegExpMatchArray | null;
      if (ident !== null) {
        const word = ident[0];
        if (state.expectsOperation) {
          if (stream.peek() === ':') {
            return 'labelDef';
          }
          state.expectsOperation = false;
          return operation(word, known);
        }
        return known.registers.has(word.toLowerCase()) ? 'register' : 'labelRef';
      }
      const symbol = stream.next();
      if (symbol === undefined) {
        return null;
      }
      return '+-*/&|^~<>'.includes(symbol) ? 'operator' : 'punctuation';
    },
    tokenTable: TOKEN_TAGS,
  };
}

const TOKEN_TAGS: Record<RiscvToken, Tag> = {
  comment: tags.lineComment,
  instruction: tags.keyword,
  pseudo: tags.macroName,
  directive: tags.processingInstruction,
  register: tags.special(tags.variableName),
  labelDef: tags.definition(tags.labelName),
  labelRef: tags.labelName,
  modifier: tags.modifier,
  number: tags.number,
  string: tags.string,
  character: tags.character,
  operator: tags.operator,
  punctuation: tags.punctuation,
  invalid: tags.invalid,
};

export function riscvLanguage(vocabulary: Vocabulary): StreamLanguage<RiscvState> {
  return StreamLanguage.define(riscvParser(vocabulary));
}
