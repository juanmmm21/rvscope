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

test('la ruta de datos de 5 etapas enseña el salto, el adelantamiento y lo vaciado', async ({
  page,
}) => {
  await open(page);
  await page.locator('#model').selectOption('5-etapas-forwarding');
  const caption = page.locator('#datapath-caption');
  await expect(page.locator('#datapath')).toBeVisible();
  await expect(page.locator('.dp-latch')).toHaveCount(4);
  await expect(page.locator('.dp-mux')).toHaveCount(6);
  await expect(caption).toHaveText('Todavía no se ha ejecutado ningún ciclo.');

  // Ciclo 6: `jalr` (del `call`) salta desde EX con `ra` adelantado de EX/MEM.
  await stepTo(page, 0, 6);
  await expect(caption).toContainText('EX jalr ra, 16(ra)');
  await expect(caption).toContainText('operando A adelantado desde EX/MEM');
  await expect(page.locator('.dp-stage[data-stage="if"]')).toHaveAttribute(
    'data-condition',
    'flushed',
  );
  await expect(page.locator('.dp-stage[data-stage="id"]')).toContainText('ID · vaciada');
  await expect(wire(page, 'ex_alu_to_pc')).toHaveAttribute('data-active', 'true');
  await expect(wire(page, 'forward_mem_a')).toHaveAttribute('data-active', 'true');
  await expect(wire(page, 'ex_rs1')).toHaveAttribute('data-active', 'false');
  await expect(wire(page, 'wb_write_back').locator('.dp-value')).toHaveText('0x00000006');

  // Paso atrás: el `auipc` anterior aún no salta y el PC sigue con pc + 4.
  await page.locator('#step-back').click();
  await expect(caption).toContainText('EX auipc ra, 0x0');
  await expect(wire(page, 'if_sequential')).toHaveAttribute('data-active', 'true');
  await expect(wire(page, 'ex_alu_to_pc')).toHaveAttribute('data-active', 'false');
});

test('sin forwarding no hay multiplexores de adelantamiento y las burbujas se ven', async ({
  page,
}) => {
  await open(page);
  await page.locator('#model').selectOption('5-etapas-burbujas');
  await expect(page.locator('.dp-mux')).toHaveCount(4);
  await expect(wire(page, 'forward_mem_a')).toHaveCount(0);
  // Ciclo 3: `lw` espera en ID al `auipc` que calcula su dirección.
  await stepTo(page, 0, 3);
  await expect(page.locator('#datapath-caption')).toContainText(
    'ID lw a0, 0(a0) (retenida) · EX auipc a0, 0x10000 · MEM burbuja',
  );
  await expect(wire(page, 'next_pc')).toHaveAttribute('data-active', 'false');
  await page.locator('#model').selectOption('5-etapas-forwarding');
  await expect(page.locator('.dp-mux')).toHaveCount(6);
});

test('la doble emisión apila sus dos vías y dice de qué vía llega lo adelantado', async ({
  page,
}) => {
  await open(page);
  await page.locator('#model').selectOption('2-vias');
  const lanes = page.locator('#datapath-figure svg.dp-pipeline');
  await expect(lanes).toHaveCount(2);
  const [old, young] = [lanes.nth(0), lanes.nth(1)];
  await expect(old.locator('.dp-lane')).toHaveText('Vía 0');
  await expect(young.locator('.dp-lane')).toHaveText('Vía 1');
  await expect(page.locator('.dp-latch')).toHaveCount(8);

  // Ciclo 5: el `jalr` del `call` va en la vía 0 y toma `ra` del `auipc`, que iba en la vía 1.
  await stepTo(page, 0, 5);
  const caption = page.locator('#datapath-caption');
  await expect(caption).toContainText('Vía 0: ');
  await expect(caption).toContainText('operando A adelantado desde EX/MEM de la vía 1');
  await expect(old.locator('.dp-stage[data-stage="ex"]')).toContainText('jalr');
  await expect(old.locator('.dp-source').first()).toHaveText('vía 1');
  await expect(old.locator('.dp-source').first()).toHaveAttribute('data-cross', 'true');
  await expect(old.locator('[data-wire="forward_mem_a"]')).toHaveAttribute('data-active', 'false');
  // El salto sale de la vía 0: solo ella escribe el PC.
  await expect(old.locator('[data-wire="ex_alu_to_pc"]')).toHaveAttribute('data-active', 'true');
  await expect(young.locator('[data-wire="next_pc"]')).toHaveAttribute('data-active', 'false');

  // Al volver a un modelo de una vía queda un solo diagrama.
  await page.locator('#model').selectOption('5-etapas-forwarding');
  await expect(lanes).toHaveCount(1);
  await expect(page.locator('.dp-lane')).toHaveCount(0);
});
