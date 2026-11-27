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

async function open(page: Page, model: string): Promise<void> {
  await page.goto('/');
  await expect(page.locator('#assembly-status')).toContainText('ensamblado');
  await expect(page.locator('#run-status')).toContainText('listo');
  await page.locator('#model').selectOption(model);
}

async function stepTo(page: Page, from: number, cycle: number): Promise<void> {
  for (let index = from; index < cycle; index += 1) {
    await page.locator('#step').click();
  }
  await expect(page.locator('#run-status')).toContainText(`ciclo ${String(cycle)} ·`);
}

// Lo que se ve de las celdas ocupadas de una fila (sin la frase para lectores de pantalla).
function stages(locator: ReturnType<Page['locator']>, state = '') {
  return locator.locator(`td[data-state${state}] > [aria-hidden="true"]`);
}

// Fila de la tabla cuya instrucción empieza por `text` (la `n`-ésima si se repite).
function row(page: Page, text: string, n = 0) {
  return page
    .locator('#pipeline-table tbody tr')
    .filter({ has: page.locator('.pt-text', { hasText: text }) })
    .nth(n);
}

test('la tabla del pipeline sigue al paso y al paso atrás con forwarding', async ({ page }) => {
  await open(page, '5-etapas-forwarding');
  const section = page.locator('#pipeline');
  const caption = page.locator('#pipeline-caption');
  await expect(section).toBeVisible();
  await expect(caption).toHaveText('Todavía no ha pasado ningún ciclo.');
  await expect(page.locator('#pipeline-table tbody tr')).toHaveCount(0);

  // Factorial: auipc + lw (`lw a0, n`), auipc + jalr (`call`). En el ciclo 6 el `jalr` está en
  // EX con `ra` adelantado desde EX/MEM y vacía las dos que se buscaron detrás.
  await stepTo(page, 0, 6);
  await expect(page.locator('#pipeline-table thead th[data-current="true"]')).toHaveText('6');
  await expect(page.locator('#pipeline-table tbody tr')).toHaveCount(6);
  const jalr = row(page, 'jalr ra');
  await expect(stages(jalr)).toHaveText(['IF', 'ID', 'EX←M']);
  const ex = jalr.locator('td[data-current="true"]');
  await expect(ex).toHaveAttribute('title', 'EX en el ciclo 6, rs1 (ra) adelantado desde EX/MEM');
  await expect(stages(row(page, 'addi a7'), '="flushed"')).toHaveText('ID');
  await expect(stages(row(page, 'ecall'), '="flushed"')).toHaveText('IF');
  await expect(caption).toHaveText(
    'Ciclos 1 a 6: 6 instrucciones · 0 ciclos retenidos · 2 vaciadas · 2 operandos adelantados',
  );

  await page.locator('#step-back').click();
  await expect(page.locator('#pipeline-table thead th[data-current="true"]')).toHaveText('5');
  await expect(stages(row(page, 'jalr ra'))).toHaveText(['IF', 'ID']);
  await expect(page.locator('#pipeline-table td[data-state="flushed"]')).toHaveCount(0);

  // El de un ciclo no tiene tabla: cada instrucción ocuparía una sola celda.
  await page.locator('#model').selectOption('un-ciclo');
  await expect(section).toBeHidden();
});

test('la tabla marca las retenidas con burbujas y las vías con doble emisión', async ({ page }) => {
  await open(page, '5-etapas-burbujas');
  // `lw a0, 0(a0)` lee el a0 que escribe el `auipc` de delante: espera dos ciclos en ID.
  await stepTo(page, 0, 6);
  const load = row(page, 'lw a0');
  await expect(stages(load)).toHaveText(['IF', 'ID', 'ID', 'ID', 'EX']);
  await expect(load.locator('td[data-state="stalled"]')).toHaveCount(2);
  await expect(load.locator('td[data-state="stalled"]').first()).toHaveAttribute(
    'title',
    'ID en el ciclo 3, retenida',
  );

  await page.locator('#model').selectOption('2-vias');
  await stepTo(page, 0, 5);
  // Las dos primeras se buscan juntas; el `lw` espera en ID a su pareja y pasa a la vía 0.
  await expect(stages(row(page, 'auipc a0'))).toHaveText(['IF0', 'ID0', 'EX0', 'MEM0', 'WB0']);
  await expect(stages(row(page, 'lw a0'))).toHaveText(['IF1', 'ID1', 'ID0', 'EX0←M0', 'MEM0']);
  await expect(row(page, 'jalr ra').locator('td[data-current="true"]')).toHaveAttribute(
    'title',
    'EX en el ciclo 5, vía 0, rs1 (ra) adelantado desde EX/MEM de la vía 1',
  );
});

test('la tabla sigue al último ciclo al ejecutar y pasa axe en los dos temas', async ({ page }) => {
  await open(page, '5-etapas-forwarding');
  await page.locator('#speed').fill((await page.locator('#speed').getAttribute('max')) ?? '8');
  await page.locator('#run').click();
  await expect(page.locator('#run-status')).toContainText('terminado');
  const header = page.locator('#pipeline-table thead th[scope="col"]');
  // Solo los últimos 32 ciclos, y el último es el actual.
  await expect(header).toHaveCount(33);
  const status = (await page.locator('#run-status').textContent()) ?? '';
  const cycles = /ciclo (\d+)/.exec(status)?.[1] ?? '';
  await expect(page.locator('#pipeline-table thead th[data-current="true"]')).toHaveText(cycles);
  const scroller = page.locator('#pipeline-scroll');
  const atEnd = await scroller.evaluate(
    (element) => element.scrollLeft + element.clientWidth >= element.scrollWidth - 4,
  );
  expect(atEnd).toBe(true);

  for (const scheme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    const result = await new AxeBuilder({ page })
      .include('#pipeline')
      .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
      .analyze();
    const found = result.violations.map(
      (violation) =>
        `${violation.id}: ${violation.nodes.map((node) => node.target.join(' ')).join(', ')}`,
    );
    expect(found, scheme).toEqual([]);
  }
});
