import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

// Ningún flujo puede dejar errores en la consola del navegador.
let errors: string[] = [];

test.beforeEach(({ page }) => {
  errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  page.on('console', (message) => {
    if (message.type() === 'error') {
      errors.push(message.text());
    }
  });
});

test.afterEach(() => {
  expect(errors).toEqual([]);
});

async function open(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.locator('#assembly-status')).toContainText('ensamblado');
  await expect(page.locator('#run-status')).toContainText('listo');
}

async function expectAccessible(page: Page, context: string): Promise<void> {
  const result = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
    .analyze();
  const found = result.violations.map(
    (violation) =>
      `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`,
  );
  expect(found, context).toEqual([]);
}

function background(page: Page): Promise<string> {
  return page.evaluate(() => getComputedStyle(document.body).backgroundColor);
}

for (const scheme of ['dark', 'light'] as const) {
  test(`sin fallos de accesibilidad ni de contraste en tema ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await open(page);
    await expectAccessible(page, 'al cargar');

    // Línea en curso, registros cambiados y lo escrito en memoria.
    for (let index = 0; index < 6; index += 1) {
      await page.locator('#step').click();
    }
    await expect(page.locator('#run-status')).toContainText('ciclo 6 ·');
    await expectAccessible(page, 'a mitad de ejecución');

    await page.locator('#model').selectOption('5-etapas-sin-riesgos');
    await expect(page.locator('#model-note')).toHaveAttribute('data-warning', 'true');
    await page.locator('.cm-content').fill('addi a0, zero, 5000\n');
    await expect(page.locator('#assembly-status')).toHaveAttribute('data-state', 'error');
    await expectAccessible(page, 'con aviso de modelo y error de ensamblado');
  });
}

test('el tema se puede fijar, se recuerda y vuelve al del sistema', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await open(page);
  expect(await background(page)).toBe('rgb(10, 11, 12)');
  await expect(page.getByRole('radio', { name: 'Sistema' })).toBeChecked();

  await page.getByText('Claro', { exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  expect(await background(page)).toBe('rgb(241, 241, 236)');

  // Se aplica antes de pintar, sin esperar al núcleo.
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.getByRole('radio', { name: 'Claro' })).toBeChecked();
  expect(await background(page)).toBe('rgb(241, 241, 236)');

  await page.getByText('Sistema', { exact: true }).click();
  await expect(page.locator('html')).not.toHaveAttribute('data-theme');
  expect(await page.evaluate(() => localStorage.getItem('rvscope.theme'))).toBeNull();
  expect(await background(page)).toBe('rgb(10, 11, 12)');
  await page.emulateMedia({ colorScheme: 'light' });
  expect(await background(page)).toBe('rgb(241, 241, 236)');
});

interface Focused {
  readonly key: string;
  readonly ring: boolean;
}

// Describe el elemento con el foco y si se ve el anillo: en él, en la etiqueta visible de una
// opción (la radio está oculta) o en el marco del editor.
function focused(page: Page): Promise<Focused | null> {
  return page.evaluate(() => {
    const active = document.activeElement;
    if (!(active instanceof HTMLElement) || active === document.body) {
      return null;
    }
    const ringed = (element: Element | null): boolean => {
      if (element === null) {
        return false;
      }
      const style = getComputedStyle(element);
      return style.outlineStyle !== 'none' && Number.parseFloat(style.outlineWidth) >= 2;
    };
    const target =
      active instanceof HTMLInputElement && active.type === 'radio'
        ? active.nextElementSibling
        : active.classList.contains('cm-content')
          ? active.closest('.source-body')
          : active;
    const key =
      active.id !== ''
        ? `#${active.id}`
        : active instanceof HTMLInputElement && active.type === 'radio'
          ? `${active.name}=${active.value}`
          : active.className;
    return { key, ring: ringed(target) };
  });
}

test('toda la página se recorre con el teclado y el foco siempre se ve', async ({ page }) => {
  await open(page);
  // Un grupo de radios es una sola parada de Tab (las flechas eligen dentro).
  const expected = [
    'skip-link',
    'theme=system',
    '#model',
    '#assemble',
    '#reset',
    '#step',
    '#run',
    '#speed',
    'cm-content',
    'memory-unit=word',
    'memory-region=stack',
    '#memory-follow-sp',
    '#memory-goto-input',
    '#memory-goto-button',
    '#memory-previous',
    '#memory-next',
    '#memory-grid',
    'register-format=hex',
    '#console-output',
    '#console-input',
    '#console-close',
  ];
  const seen: string[] = [];
  for (let index = 0; index < 40; index += 1) {
    const before = await focused(page);
    // Dentro del editor Tab indenta: se sale con Escape y luego Tab.
    if (before?.key === 'cm-content') {
      await page.keyboard.press('Escape');
    }
    await page.keyboard.press('Tab');
    const now = await focused(page);
    if (now === null) {
      break;
    }
    expect(now.ring, `anillo de foco en ${now.key}`).toBe(true);
    if (seen.includes(now.key)) {
      break;
    }
    seen.push(now.key);
  }
  for (const key of expected) {
    expect(seen, `se llega a ${key}`).toContain(key);
  }
  expect(seen.indexOf('skip-link')).toBe(0);
});

test('el enlace de salto lleva el foco al editor', async ({ page }) => {
  await open(page);
  await page.keyboard.press('Tab');
  await expect(page.locator('.skip-link')).toBeFocused();
  await expect(page.locator('.skip-link')).toBeInViewport();
  await page.keyboard.press('Enter');
  await expect(page.locator('.cm-content')).toBeFocused();
  await page.keyboard.press('F10');
  await expect(page.locator('#run-status')).toContainText('ciclo 1 ·');
});

test('en un móvil no hay scroll horizontal y todo sigue a mano', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 780 });
  await open(page);
  const overflow = await page.evaluate(() => {
    const root = document.documentElement;
    return root.scrollWidth - root.clientWidth;
  });
  expect(overflow).toBeLessThanOrEqual(0);
  await expect(page.locator('#listing')).toBeHidden();
  for (const id of ['#model', '#step', '#run', '#speed', '#memory-goto-input', '#console-input']) {
    const box = await page.locator(id).boundingBox();
    expect(box, id).not.toBeNull();
    if (box !== null) {
      expect(box.x, id).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width, id).toBeLessThanOrEqual(360);
    }
  }
  await page.locator('#step').click();
  await expect(page.locator('#run-status')).toContainText('ciclo 1 ·');
  await expectAccessible(page, 'en pantalla estrecha');
});

test('con movimiento reducido no hay transiciones', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await open(page);
  const durations = await page.evaluate(() =>
    [...document.querySelectorAll('*')]
      .map((element) => getComputedStyle(element).transitionDuration)
      .filter((duration) => duration.split(',').some((part) => Number.parseFloat(part) > 0)),
  );
  expect(durations).toEqual([]);
});
