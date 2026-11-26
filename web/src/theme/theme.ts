// Tema: por defecto el del sistema operativo; se puede fijar claro u oscuro y se recuerda.
// `index.html` aplica lo guardado con un script en línea antes de pintar (si no, al cargar se
// vería un destello del otro tema); usa la misma clave y los mismos valores que aquí.

export type ThemeChoice = 'system' | 'light' | 'dark';

export const THEME_KEY = 'rvscope.theme';

export const THEME_CHOICES: readonly {
  readonly id: ThemeChoice;
  readonly label: string;
  readonly description: string;
}[] = [
  { id: 'system', label: 'Sistema', description: 'Seguir el tema del sistema operativo' },
  { id: 'light', label: 'Claro', description: 'Tema claro' },
  { id: 'dark', label: 'Oscuro', description: 'Tema oscuro' },
];

export function parseThemeChoice(value: string | null): ThemeChoice {
  return value === 'light' || value === 'dark' ? value : 'system';
}

export interface ThemeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

// Se pide el almacenamiento con una función porque ya leer `window.localStorage` puede lanzar
// (cookies bloqueadas), no solo usarlo.
export function readTheme(storage: () => ThemeStorage): ThemeChoice {
  try {
    return parseThemeChoice(storage().getItem(THEME_KEY));
  } catch (error: unknown) {
    console.warn('rvscope: no se pudo leer el tema guardado', error);
    return 'system';
  }
}

export function storeTheme(storage: () => ThemeStorage, choice: ThemeChoice): void {
  try {
    if (choice === 'system') {
      storage().removeItem(THEME_KEY);
    } else {
      storage().setItem(THEME_KEY, choice);
    }
  } catch (error: unknown) {
    console.warn('rvscope: no se pudo guardar el tema', error);
  }
}

/** Sin atributo manda `color-scheme: light dark`, es decir, el sistema. */
export function applyTheme(root: HTMLElement, choice: ThemeChoice): void {
  if (choice === 'system') {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = choice;
  }
}
