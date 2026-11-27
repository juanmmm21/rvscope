import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

// Ficha de la chuleta dentro del editor: con el ratón, con el cursor de texto y F2.

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

// Lleva el cursor con el teclado al principio del texto de la línea `line` (1 = primera).
async function cursorToLine(page: Page, line: number): Promise<void> {
  await page.locator('.cm-content').click();
  await page.keyboard.press('Control+Home');
  for (let index = 1; index < line; index += 1) {
    await page.keyboard.press('ArrowDown');
  }
  await page.keyboard.press('End');
  await page.keyboard.press('Home');
}

const card = (page: Page) => page.locator('.rf-card');

test('al pasar el ratón por una instrucción sale su ficha y por un operando no', async ({
  page,
}) => {
  await open(page);
  await page
    .locator('.cm-line .tok-instruction', { hasText: /^addi$/u })
    .first()
    .hover();
  await expect(card(page)).toBeVisible();
  await expect(card(page)).toContainText('addi rd, rs1, imm');
  await expect(card(page)).toContainText('formato I');
  await expect(card(page)).toContainText('Formato I · base RV32I');

  await page.locator('.cm-line .tok-register', { hasText: /^sp$/u }).first().hover();
  await expect(card(page)).toHaveCount(0);
});

test('con el cursor de texto sale la ficha, se quita al escribir y con Escape', async ({
  page,
}) => {
  await open(page);
  // Línea 7: «lw a0, n», instrucción y también pseudoinstrucción con etiqueta.
  await cursorToLine(page, 7);
  await expect(card(page)).toBeVisible();
  await expect(card(page).locator('.rf-entry')).toHaveCount(2);
  await expect(card(page).locator('.rf-entry').first()).toContainText('lw rd, offset(rs1)');
  await expect(card(page).locator('.rf-entry').last()).toContainText('pseudoinstrucción');

  await page.keyboard.press('Escape');
  await expect(card(page)).toHaveCount(0);

  await page.keyboard.press('ArrowRight');
  await expect(card(page)).toBeVisible();
  await page.keyboard.type('x');
  await expect(card(page)).toHaveCount(0);

  // Directivas también: línea 3 «n: .word 6».
  await cursorToLine(page, 3);
  await page.keyboard.press('End');
  await page.keyboard.press('Control+ArrowLeft');
  await page.keyboard.press('Control+ArrowLeft');
  await expect(card(page)).toContainText('directiva');
});

test('F2 sobre una instrucción abre la chuleta en su ficha y Escape vuelve al editor', async ({
  page,
}) => {
  await open(page);
  await page.locator('#cheatsheet-toggle').click();
  await page.locator('#cheatsheet-search').fill('zzz');
  await page.keyboard.press('F2');
  await expect(page.locator('#cheatsheet')).toBeHidden();

  await cursorToLine(page, 7);
  await page.keyboard.press('F2');
  await expect(page.locator('#cheatsheet')).toBeVisible();
  // La búsqueda anterior se quita para que la ficha esté a la vista.
  await expect(page.locator('#cheatsheet-search')).toHaveValue('');
  const button = page.locator('.cs-entry[data-instruction="lw"] .cs-open');
  await expect(button).toBeFocused();
  await expect(button).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('.cs-entry[data-instruction="lw"] .cs-card')).toBeInViewport();
  await page.keyboard.press('Escape');
  await expect(page.locator('#cheatsheet')).toBeHidden();
  await expect(page.locator('.cm-content')).toBeFocused();

  // Una pseudoinstrucción: la fila se enfoca aunque no tenga botón.
  await cursorToLine(page, 9);
  await page.keyboard.press('F2');
  await expect(
    page.locator('.cs-section[data-kind="pseudo"] .cs-entry[data-name="li"]'),
  ).toBeFocused();
});

for (const scheme of ['dark', 'light'] as const) {
  test(`ficha del editor sin fallos de accesibilidad en tema ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await open(page);
    await cursorToLine(page, 7);
    await expect(card(page)).toBeVisible();
    const result = await new AxeBuilder({ page })
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
      .analyze();
    const found = result.violations.map(
      (violation) =>
        `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`,
    );
    expect(found).toEqual([]);
  });
}
