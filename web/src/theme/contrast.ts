// Contraste WCAG 2.x entre colores del tema. Los tokens se escriben en `styles.css` como
// `--nombre: light-dark(#claro, #oscuro);` para que cada color tenga un único sitio y aquí se
// puedan medir los dos temas sin un navegador.

export type Scheme = 'light' | 'dark';

export interface ThemeColor {
  readonly light: string;
  readonly dark: string;
}

export type ThemeTokens = ReadonlyMap<string, ThemeColor>;

interface Rgb {
  readonly r: number;
  readonly g: number;
  readonly b: number;
}

const HEX = /^#([0-9a-f]{6})$/i;
const TOKEN = /--([a-z0-9-]+)\s*:\s*light-dark\(\s*(#[0-9a-f]{6})\s*,\s*(#[0-9a-f]{6})\s*\)/gi;

function parseHex(color: string): Rgb {
  const match = HEX.exec(color);
  if (match === null) {
    throw new Error(`color no admitido: ${color} (se espera #rrggbb)`);
  }
  const [, digits] = match;
  if (digits === undefined) {
    throw new Error(`color no admitido: ${color}`);
  }
  const value = Number.parseInt(digits, 16);
  return { r: (value >> 16) & 0xff, g: (value >> 8) & 0xff, b: value & 0xff };
}

function toHex({ r, g, b }: Rgb): string {
  return `#${[r, g, b].map((channel) => channel.toString(16).padStart(2, '0')).join('')}`;
}

function linear(channel: number): number {
  const value = channel / 255;
  return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

export function relativeLuminance(color: string): number {
  const { r, g, b } = parseHex(color);
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

export function contrastRatio(a: string, b: string): number {
  const first = relativeLuminance(a);
  const second = relativeLuminance(b);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

/** Lo que pinta `color-mix(in srgb, color weight, base)` sobre un fondo opaco. */
export function mix(color: string, base: string, weight: number): string {
  if (weight < 0 || weight > 1) {
    throw new Error(`peso fuera de [0, 1]: ${String(weight)}`);
  }
  const top = parseHex(color);
  const bottom = parseHex(base);
  const blend = (a: number, b: number): number => Math.round(a * weight + b * (1 - weight));
  return toHex({ r: blend(top.r, bottom.r), g: blend(top.g, bottom.g), b: blend(top.b, bottom.b) });
}

export function parseThemeTokens(css: string): ThemeTokens {
  const tokens = new Map<string, ThemeColor>();
  for (const match of css.matchAll(TOKEN)) {
    const [, name, light, dark] = match;
    if (name === undefined || light === undefined || dark === undefined) {
      continue;
    }
    if (tokens.has(name)) {
      throw new Error(`token repetido: --${name}`);
    }
    tokens.set(name, { light: light.toLowerCase(), dark: dark.toLowerCase() });
  }
  return tokens;
}

export function tokenColor(tokens: ThemeTokens, name: string, scheme: Scheme): string {
  const token = tokens.get(name);
  if (token === undefined) {
    throw new Error(`falta el token --${name}`);
  }
  return token[scheme];
}
