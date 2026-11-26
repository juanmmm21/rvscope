import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { contrastRatio, mix, parseThemeTokens, tokenColor, type Scheme } from './contrast';

// Vitest vacía los `.css` importados aunque se pidan `?raw`: se lee el fichero tal cual.
const css = readFileSync(new URL('../styles.css', import.meta.url), 'utf8');
const tokens = parseThemeTokens(css);
const SCHEMES: readonly Scheme[] = ['light', 'dark'];

// Texto (AA normal, 4.5:1) sobre cada fondo donde aparece: la página, las superficies (línea
// activa, hover, lectura de memoria) y los tintes de la línea en curso (acento al 10 %) y del
// fallo (peligro al 12 %), como en `styles.css`.
const TEXT = [
  'fg',
  'muted',
  'accent',
  'danger',
  'syn-instruction',
  'syn-pseudo',
  'syn-directive',
  'syn-register',
  'syn-number',
  'syn-string',
  'syn-label',
];

// Dentro del editor no hay texto en pistacho: la selección y las líneas en curso o con fallo
// solo llevan los colores de sintaxis, el apagado de los comentarios y el de error.
const EDITOR_TEXT = TEXT.filter((name) => name !== 'accent');

function backgrounds(scheme: Scheme): Record<string, { color: string; text: readonly string[] }> {
  const bg = tokenColor(tokens, 'bg', scheme);
  return {
    bg: { color: bg, text: TEXT },
    surface: { color: tokenColor(tokens, 'surface', scheme), text: TEXT },
    // Línea en curso del editor y lo escrito en memoria (pistacho sobre su propio tinte).
    'accent-tint': { color: mix(tokenColor(tokens, 'accent', scheme), bg, 0.1), text: TEXT },
    selection: { color: tokenColor(tokens, 'selection', scheme), text: EDITOR_TEXT },
    'fault-line': {
      color: mix(tokenColor(tokens, 'danger', scheme), bg, 0.12),
      text: EDITOR_TEXT,
    },
  };
}

describe('contrastRatio', () => {
  it('da los extremos de WCAG', () => {
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 5);
    expect(contrastRatio('#777777', '#777777')).toBeCloseTo(1, 5);
    expect(contrastRatio('#767676', '#ffffff')).toBeCloseTo(4.54, 2);
  });

  it('rechaza colores que no son #rrggbb', () => {
    expect(() => contrastRatio('red', '#ffffff')).toThrow(/no admitido/);
  });
});

describe('mix', () => {
  it('mezcla en sRGB como color-mix', () => {
    expect(mix('#ffffff', '#000000', 0.5)).toBe('#808080');
    expect(mix('#c4f27e', '#0a0b0c', 0)).toBe('#0a0b0c');
    expect(() => mix('#ffffff', '#000000', 2)).toThrow(/peso/);
  });
});

describe('tokens del tema', () => {
  it('lee cada token con sus dos colores', () => {
    expect(tokens.get('bg')).toEqual({ light: '#f1f1ec', dark: '#0a0b0c' });
    for (const name of [...TEXT, 'bg', 'surface', 'line', 'line-strong', 'selection']) {
      expect(tokens.has(name), `--${name}`).toBe(true);
    }
  });

  it('rechaza un token repetido', () => {
    expect(() =>
      parseThemeTokens(
        ':root{--a: light-dark(#000000, #ffffff); --a: light-dark(#000000, #ffffff)}',
      ),
    ).toThrow(/repetido/);
  });

  for (const scheme of SCHEMES) {
    for (const [background, { color, text }] of Object.entries(backgrounds(scheme))) {
      it(`${scheme}: texto AA sobre ${background}`, () => {
        for (const name of text) {
          const ratio = contrastRatio(tokenColor(tokens, name, scheme), color);
          expect(ratio, `--${name} sobre ${background}`).toBeGreaterThanOrEqual(4.5);
        }
      });
    }

    it(`${scheme}: bordes de campos, foco y marcas a 3:1`, () => {
      const bg = tokenColor(tokens, 'bg', scheme);
      for (const name of ['line-strong', 'accent', 'danger', 'muted']) {
        expect(
          contrastRatio(tokenColor(tokens, name, scheme), bg),
          `--${name}`,
        ).toBeGreaterThanOrEqual(3);
      }
    });
  }
});
