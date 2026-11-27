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

const SECTIONS = [
  'Aritméticas',
  'Lógicas',
  'Desplazamientos',
  'Comparaciones',
  'Inmediatos superiores',
  'Cargas',
  'Almacenamientos',
  'Saltos condicionales',
  'Saltos incondicionales',
  'Multiplicación (M)',
  'División (M)',
  'Sistema',
  'Pseudoinstrucciones',
  'Directivas',
  'Llamadas al sistema (ecall)',
  'Registros y convenio ABI',
];

test('la chuleta trae todas las instrucciones, pseudoinstrucciones, ecalls y registros', async ({
  page,
}) => {
  await open(page);
  const drawer = page.locator('#cheatsheet');
  await expect(drawer).toBeHidden();
  await page.locator('#cheatsheet-toggle').click();
  await expect(drawer).toBeVisible();
  await expect(page.locator('#cheatsheet-toggle')).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('#cheatsheet-title')).toBeFocused();

  await expect(page.locator('#cheatsheet-nav a')).toHaveText(SECTIONS);
  await expect(page.locator('.cs-heading')).toHaveText(SECTIONS);
  // RV32I (40) + M (8), cada una una sola vez.
  const instructions = page.locator('.cs-section[data-kind="instructions"] .cs-mnemonic');
  await expect(instructions).toHaveCount(48);
  const names = await instructions.allTextContents();
  expect(new Set(names).size).toBe(48);
  for (const name of ['lui', 'jalr', 'sltiu', 'srai', 'mulhsu', 'remu', 'ecall', 'fence']) {
    expect(names, name).toContain(name);
  }
  await expect(page.locator('.cs-section[data-kind="pseudo"] .cs-entry')).toHaveCount(36);
  await expect(page.locator('.cs-section[data-kind="directives"] .cs-entry')).toHaveCount(23);
  await expect(page.locator('.cs-registers tbody tr')).toHaveCount(32);

  const addi = page.locator('.cs-entry', {
    has: page.locator('.cs-mnemonic', { hasText: /^addi$/u }),
  });
  await expect(addi.locator('dt')).toContainText('addi rd, rs1, imm');
  await expect(addi.locator('.cs-tag')).toHaveText('formato I');
  const li = page.locator('.cs-section[data-kind="pseudo"] .cs-entry').filter({
    has: page.locator('.cs-mnemonic', { hasText: /^li$/u }),
  });
  await expect(li.locator('.cs-operation')).toHaveCount(2);
  await expect(page.locator('.cs-section[data-kind="syscalls"]')).toContainText(
    'a7 = 5 · → a0 = entero',
  );
  const sp = page.locator('.cs-registers tbody tr', { hasText: 'Puntero de pila' });
  await expect(sp).toContainText('x2');
  await expect(sp).toContainText('lo guarda la función');
});

test('el índice lleva a cada sección sin tocar la dirección de la página', async ({ page }) => {
  await open(page);
  await page.locator('#cheatsheet-toggle').click();
  await page.locator('#cheatsheet-nav a', { hasText: 'Registros y convenio ABI' }).click();
  const heading = page.locator('#cs-registers');
  await expect(heading).toBeFocused();
  await expect(heading).toBeInViewport();
  expect(new URL(page.url()).hash).toBe('');
});

test('F2 la abre y cierra desde el editor, y Escape devuelve el foco', async ({ page }) => {
  await open(page);
  await page.locator('.cm-content').click();
  await page.keyboard.press('F2');
  await expect(page.locator('#cheatsheet')).toBeVisible();
  await expect(page.locator('#cheatsheet-title')).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(page.locator('#cheatsheet')).toBeHidden();
  await expect(page.locator('#cheatsheet-toggle')).toHaveAttribute('aria-expanded', 'false');
  await expect(page.locator('.cm-content')).toBeFocused();

  // Abierta, se puede volver al editor y seguir escribiendo; F2 la cierra sin mover el foco.
  await page.keyboard.press('F2');
  await page.locator('.cm-content').click();
  await page.keyboard.type('# hola');
  await expect(page.locator('#cheatsheet')).toBeVisible();
  await page.keyboard.press('F2');
  await expect(page.locator('#cheatsheet')).toBeHidden();
  await expect(page.locator('.cm-content')).toBeFocused();

  // El botón de cerrar devuelve el foco al botón que la abrió.
  await page.locator('#cheatsheet-toggle').click();
  await page.locator('#cheatsheet-close').click();
  await expect(page.locator('#cheatsheet-toggle')).toBeFocused();
});

for (const scheme of ['dark', 'light'] as const) {
  test(`chuleta abierta sin fallos de accesibilidad en tema ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await open(page);
    await page.locator('#cheatsheet-toggle').click();
    await expectAccessible(page, 'chuleta abierta');
  });
}

test('en un móvil ocupa el ancho sin scroll horizontal', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 780 });
  await open(page);
  await page.locator('#cheatsheet-toggle').click();
  const box = await page.locator('#cheatsheet').boundingBox();
  expect(box).not.toBeNull();
  if (box !== null) {
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(360);
  }
  const overflow = await page.evaluate(() => {
    const body = document.querySelector('#cheatsheet-body');
    return body === null ? -1 : body.scrollWidth - body.clientWidth;
  });
  expect(overflow).toBe(0);
  await expectAccessible(page, 'chuleta en pantalla estrecha');
});
