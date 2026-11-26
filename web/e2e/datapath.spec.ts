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

function wire(page: Page, id: string) {
  return page.locator(`.dp-wire[data-wire="${id}"]`);
}

async function stepTo(page: Page, from: number, cycle: number): Promise<void> {
  for (let index = from; index < cycle; index += 1) {
    await page.locator('#step').click();
  }
  await expect(page.locator('#run-status')).toContainText(`ciclo ${String(cycle)} ·`);
}

test('la ruta de datos del de un ciclo sigue al paso y al paso atrás', async ({ page }) => {
  await open(page);
  const figure = page.locator('#datapath');
  const caption = page.locator('#datapath-caption');
  await expect(figure).toBeVisible();
  await expect(caption).toHaveText('Todavía no se ha ejecutado ninguna instrucción.');
  await expect(page.locator('.dp-wire[data-active="true"]')).toHaveCount(0);

  // Factorial: auipc + lw (`lw a0, n`), auipc + jalr (`call`), addi sp, sw ra.
  await stepTo(page, 0, 6);
  await expect(caption).toContainText('sw ra, 4(sp)');
  await expect(caption).toContainText('escribe memoria');
  await expect(wire(page, 'store_data')).toHaveAttribute('data-active', 'true');
  await expect(wire(page, 'rs2')).toHaveAttribute('data-active', 'false');
  await expect(wire(page, 'write_back')).toHaveAttribute('data-active', 'false');
  await expect(page.locator('.dp-instruction')).toHaveText('sw ra, 4(sp)');
  await expect(page.locator('.dp-value[data-written="true"]')).toHaveText(/^0x[0-9a-f]{8}$/);

  await page.locator('#step-back').click();
  await expect(caption).toContainText('addi sp, sp, -8');
  await expect(caption).toContainText('sp = ');
  await expect(wire(page, 'write_back')).toHaveAttribute('data-active', 'true');
  await expect(wire(page, 'write_back').locator('.dp-value')).toHaveText(/^0x7fff/);

  // `ble a0, t0, base` es `bge t0, a0`: con a0 = 6 no salta.
  await stepTo(page, 5, 9);
  await expect(caption).toContainText('bge t0, a0');
  await expect(wire(page, 'target')).toHaveAttribute('data-active', 'true');
  await expect(wire(page, 'immediate')).toHaveAttribute('data-active', 'false');
  await expect(wire(page, 'immediate_to_target')).toHaveAttribute('data-active', 'true');
  await expect(page.locator('.dp-mux').first().locator('.dp-selection')).toHaveAttribute(
    'points',
    '24,200 46,230',
  );
});

test('la ruta de datos se oculta con los procesadores que aún no la tienen', async ({ page }) => {
  await open(page);
  await page.locator('#model').selectOption('5-etapas-forwarding');
  await expect(page.locator('#datapath')).toBeHidden();
  await page.locator('#model').selectOption('un-ciclo');
  await expect(page.locator('#datapath')).toBeVisible();
});
