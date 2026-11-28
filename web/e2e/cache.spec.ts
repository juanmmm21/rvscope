import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Locator, type Page } from '@playwright/test';

// El panel solo se actualiza a la vista: con una ventana alta cabe entero junto a los controles.
test.use({ viewport: { width: 1280, height: 3200 } });

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
  await page.locator('#cache').scrollIntoViewIfNeeded();
}

function block(page: Page, kind: 'instruction' | 'data'): Locator {
  return page.locator(`.cache-block[data-kind="${kind}"]`);
}

// Valor de un contador por su etiqueta.
function counter(cache: Locator, label: string): Locator {
  return cache.locator(`dt:text-is("${label}") + dd`);
}

test('las cachés empiezan apagadas y cuentan el factorial paso a paso', async ({ page }) => {
  await open(page);
  const instruction = block(page, 'instruction');
  const data = block(page, 'data');
  await expect(instruction.locator('.cache-details')).toBeHidden();
  await expect(instruction.getByLabel('Conjuntos', { exact: true })).toBeDisabled();

  await instruction.getByLabel('activar').check();
  await data.getByLabel('activar').check();
  await expect(instruction.locator('.cache-geometry')).toHaveText(
    '512 bytes · 16 conjuntos × 2 vías × 16 bytes',
  );
  await expect(counter(instruction, 'accesos')).toHaveText('0');
  await expect(instruction.locator('.cache-chart figcaption')).toHaveText(
    'Todavía no hay accesos.',
  );

  // `lw a0, n` es auipc + lw: la primera búsqueda falla en frío, la segunda acierta en la
  // misma línea y la carga de `n` falla en la de datos.
  await page.locator('#step').click();
  await expect(instruction.locator('.cache-recent li')).toHaveText([
    'Lectura de 0x00000000: fallo · conjunto 0, vía 0',
  ]);
  await expect(instruction.locator('td[data-recent="miss"]')).toHaveCount(1);
  await page.locator('#step').click();
  await expect(instruction.locator('.cache-recent li')).toHaveText([
    'Lectura de 0x00000004: acierto · conjunto 0, vía 0',
  ]);
  await expect(instruction.locator('td[data-recent="hit"]')).toHaveCount(1);
  await expect(counter(instruction, 'tasa de acierto')).toHaveText('50,0 %');
  await expect(data.locator('.cache-recent li')).toHaveText([
    'Lectura de 0x10000000: fallo · conjunto 0, vía 0',
  ]);
  await expect(data.locator('.cache-address-value')).toHaveText(['0x100000', '0x0', '0x0']);

  // Paso atrás: las cuentas vuelven a las de un paso.
  await page.locator('#step-back').click();
  await expect(counter(instruction, 'accesos')).toHaveText('1');
  await expect(counter(data, 'accesos')).toHaveText('0');
  await expect(data.locator('.cache-recent li')).toHaveText(['sin accesos en el último paso']);
});

test('al ejecutar entero, la caché de instrucciones ve cada instrucción retirada', async ({
  page,
}) => {
  await open(page);
  const instruction = block(page, 'instruction');
  const data = block(page, 'data');
  await instruction.getByLabel('activar').check();
  await data.getByLabel('activar').check();
  await page.locator('#speed').fill('8');
  await page.locator('#run').click();
  await expect(page.locator('#run-status')).toContainText('terminado con código 0');
  const status = (await page.locator('#run-status').textContent()) ?? '';
  const retired = /(\d+) instrucci/.exec(status)?.[1];
  expect(retired).toBeDefined();
  await expect(counter(instruction, 'accesos')).toHaveText(retired ?? '');
  // El factorial cabe de sobra en 512 bytes: solo fallan las líneas que se tocan por primera vez.
  await expect(counter(instruction, 'expulsiones')).toHaveText('0');
  await expect(instruction.locator('.cache-chart path.cache-line')).toHaveAttribute('d', /^M.+L/);
  await expect(instruction.locator('.cache-chart figcaption')).toContainText(
    `tras ${retired ?? ''} accesos`,
  );
  await expect(counter(data, 'aciertos en escrituras')).not.toHaveText('0 de 0');
});

test('cambiar la configuración vuelve a empezar el programa', async ({ page }) => {
  await open(page);
  const data = block(page, 'data');
  await data.getByLabel('activar').check();
  for (let index = 0; index < 4; index += 1) {
    await page.locator('#step').click();
  }
  await expect(page.locator('#run-status')).toContainText('ciclo 4 ·');
  await data.getByLabel('Vías', { exact: true }).selectOption('1');
  await expect(page.locator('#run-status')).toContainText('listo');
  await expect(counter(data, 'accesos')).toHaveText('0');
  await expect(data.locator('.cache-sets thead th')).toHaveText(['Conjunto', 'Vía 0']);

  // Write-through propone quitar la asignación en fallo de escritura; se puede volver a marcar.
  await data.getByLabel('Escritura', { exact: true }).selectOption('write-through');
  await expect(data.getByLabel('asignar en fallo de escritura')).not.toBeChecked();
  await expect(counter(data, 'escrituras a memoria')).toHaveText('0');

  // Una sola vía y un solo conjunto: el índice se queda sin bits.
  await data.getByLabel('Conjuntos', { exact: true }).selectOption('1');
  await expect(data.locator('.cache-address-bits').nth(1)).toHaveText('sin bits');

  await data.getByLabel('activar').uncheck();
  await expect(data.locator('.cache-details')).toBeHidden();
});

for (const scheme of ['light', 'dark'] as const) {
  test(`el panel de caché pasa axe en modo ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await open(page);
    await block(page, 'instruction').getByLabel('activar').check();
    await block(page, 'data').getByLabel('activar').check();
    for (let index = 0; index < 3; index += 1) {
      await page.locator('#step').click();
    }
    await expect(block(page, 'data').locator('td[data-valid="true"]')).toHaveCount(1);
    const result = await new AxeBuilder({ page })
      .include('#cache')
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
      .analyze();
    const found = result.violations.map(
      (violation) =>
        `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`,
    );
    expect(found, scheme).toEqual([]);
  });
}
