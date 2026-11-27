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

// Las fichas con los campos más enrevesados: inmediato partido, funct7 en un formato I,
// funct12 fijo y los conjuntos de fence.
const TRICKY_CARDS = ['sw', 'beq', 'jal', 'srai', 'ecall', 'fence'];

async function openCards(page: Page, names: readonly string[]): Promise<void> {
  for (const name of names) {
    await page.locator(`.cs-entry[data-instruction="${name}"] .cs-open`).click();
  }
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
  const instructions = page.locator('.cs-section[data-kind="instructions"] .cs-open .cs-mnemonic');
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
    await openCards(page, TRICKY_CARDS);
    await expectAccessible(page, 'chuleta abierta con fichas');
  });
}

test('en un móvil ocupa el ancho sin scroll horizontal', async ({ page }) => {
  await page.setViewportSize({ width: 360, height: 780 });
  await open(page);
  await page.locator('#cheatsheet-toggle').click();
  await openCards(page, TRICKY_CARDS);
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

test('cada instrucción se despliega en su ficha con los campos de bits y un ejemplo', async ({
  page,
}) => {
  await open(page);
  await page.locator('#cheatsheet-toggle').click();
  const rows = page.locator('.cs-section[data-kind="instructions"] .cs-entry');
  await expect(rows.locator('.cs-open')).toHaveCount(48);
  await expect(page.locator('.cs-card:visible')).toHaveCount(0);

  const addi = page.locator('.cs-entry[data-instruction="addi"]');
  const button = addi.locator('.cs-open');
  await expect(button).toHaveAttribute('aria-expanded', 'false');
  await button.click();
  await expect(button).toHaveAttribute('aria-expanded', 'true');
  const card = addi.locator('.cs-card');
  await expect(card).toBeVisible();
  await expect(card.locator('.cs-example')).toContainText('addi sp, sp, -16');
  await expect(card.locator('.cs-example')).toContainText('0xff010113');
  await expect(card.locator('thead th')).toHaveText([
    'imm[11:0]',
    'rs1',
    'funct3 (fijo)',
    'rd',
    'opcode (fijo)',
  ]);
  await expect(card.locator('.cs-range td')).toHaveText(['31–20', '19–15', '14–12', '11–7', '6–0']);
  await expect(card.locator('.cs-bits td')).toHaveText([
    '111111110000',
    '00010',
    '000',
    '00010',
    '0010011',
  ]);
  await expect(card.locator('.cs-bits td.cs-fixed')).toHaveCount(2);

  // Con el teclado se cierra igual, y abrir una no cierra las demás.
  await button.focus();
  await page.keyboard.press('Enter');
  await expect(card).toBeHidden();
  await openCards(page, ['srai', 'ecall']);
  await expect(page.locator('.cs-card:visible')).toHaveCount(2);
  await expect(page.locator('.cs-entry[data-instruction="srai"] thead th').first()).toHaveText(
    'funct7 (fijo)',
  );
  await expect(page.locator('.cs-entry[data-instruction="srai"] .cs-bits td').first()).toHaveText(
    '0100000',
  );
  await expect(page.locator('.cs-entry[data-instruction="ecall"] .cs-bits td')).toHaveText([
    '000000000000',
    '00000',
    '000',
    '00000',
    '1110011',
  ]);
});

test('el buscador filtra la chuleta sin tildes ni mayúsculas y lleva al resultado', async ({
  page,
}) => {
  await open(page);
  await page.locator('#cheatsheet-toggle').click();
  const search = page.locator('#cheatsheet-search');
  const status = page.locator('#cheatsheet-search-status');
  const visibleSections = page.locator('.cs-section:visible .cs-heading');

  await search.fill('SRAI');
  await expect(page.locator('.cs-section[data-kind="instructions"] .cs-entry:visible')).toHaveCount(
    1,
  );
  await expect(page.locator('.cs-entry[data-instruction="srai"]')).toBeVisible();
  await expect(status).toContainText('coincidencia');
  // Enter lleva a la primera instrucción y Enter otra vez abre su ficha.
  await search.press('Enter');
  const button = page.locator('.cs-entry[data-instruction="srai"] .cs-open');
  await expect(button).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page.locator('.cs-entry[data-instruction="srai"] .cs-card')).toBeVisible();

  // Sin tildes: «division» encuentra la sección de la M, y el índice solo enseña lo que queda.
  await search.fill('division');
  await expect(visibleSections.first()).toHaveText('División (M)');
  await expect(page.locator('#cheatsheet-nav li:visible')).toHaveText(
    await visibleSections.allTextContents(),
  );

  // Pseudoinstrucciones por su expansión, registros por su uso, ecalls por su número.
  await search.fill('pila');
  await expect(page.locator('.cs-registers tbody tr:visible')).toContainText(['sp']);
  await search.fill('a7 = 5');
  await expect(visibleSections).toHaveText(['Llamadas al sistema (ecall)']);
  await search.fill('jalr x0');
  await expect(
    page.locator('.cs-section[data-kind="pseudo"] .cs-entry:visible .cs-mnemonic'),
  ).toContainText(['ret']);

  await search.fill('zzz');
  await expect(status).toHaveText('Nada coincide con «zzz»');
  await expect(visibleSections).toHaveCount(0);
  await expect(page.locator('#cheatsheet-nav li:visible')).toHaveCount(0);

  // Escape con texto borra la búsqueda y deja el panel abierto; con el campo vacío lo cierra.
  await search.press('Escape');
  await expect(search).toHaveValue('');
  await expect(status).toHaveText('');
  await expect(page.locator('.cs-heading:visible')).toHaveText(SECTIONS);
  await expect(page.locator('#cheatsheet')).toBeVisible();
  await search.press('Escape');
  await expect(page.locator('#cheatsheet')).toBeHidden();
});

for (const scheme of ['dark', 'light'] as const) {
  test(`buscador sin fallos de accesibilidad en tema ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await open(page);
    await page.locator('#cheatsheet-toggle').click();
    await page.locator('#cheatsheet-search').fill('suma');
    await expectAccessible(page, 'chuleta filtrada');
    await page.locator('#cheatsheet-search').fill('');
    await expectAccessible(page, 'buscador vacío con su texto de ayuda');
  });
}
