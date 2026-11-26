import { describe, expect, it, vi } from 'vitest';
import { THEME_KEY, parseThemeChoice, readTheme, storeTheme, type ThemeStorage } from './theme';

function memoryStorage(initial: Record<string, string> = {}): ThemeStorage & {
  readonly values: Map<string, string>;
} {
  const values = new Map(Object.entries(initial));
  return {
    values,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => void values.set(key, value),
    removeItem: (key) => void values.delete(key),
  };
}

function brokenStorage(): ThemeStorage {
  const fail = (): never => {
    throw new Error('almacenamiento no disponible');
  };
  return { getItem: fail, setItem: fail, removeItem: fail };
}

describe('tema', () => {
  it('solo acepta claro u oscuro; lo demás es el del sistema', () => {
    expect(parseThemeChoice('light')).toBe('light');
    expect(parseThemeChoice('dark')).toBe('dark');
    expect(parseThemeChoice('system')).toBe('system');
    expect(parseThemeChoice('sepia')).toBe('system');
    expect(parseThemeChoice(null)).toBe('system');
  });

  it('guarda el tema fijado y borra la clave al volver al sistema', () => {
    const storage = memoryStorage();
    storeTheme(() => storage, 'light');
    expect(storage.values.get(THEME_KEY)).toBe('light');
    expect(readTheme(() => storage)).toBe('light');
    storeTheme(() => storage, 'system');
    expect(storage.values.has(THEME_KEY)).toBe(false);
    expect(readTheme(() => storage)).toBe('system');
  });

  it('sin almacenamiento sigue al sistema y avisa', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined);
    expect(readTheme(brokenStorage)).toBe('system');
    storeTheme(brokenStorage, 'dark');
    expect(warn).toHaveBeenCalledTimes(2);
    warn.mockRestore();
  });
});
