import { expect, test, type Page } from '@playwright/test';

// Línea de `mul a0, a0, t1` en examples/factorial.s.
const MUL_LINE = 22;

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

function register(page: Page, abi: string) {
  return page.locator('.register', { has: page.locator('.register-name', { hasText: abi }) });
}

async function setSource(page: Page, source: string): Promise<void> {
  await page.locator('.cm-content').fill(source);
  await expect(page.locator('#assembly-status')).toContainText('ensamblado');
}

// El margen tiene un marcador oculto que solo le da el ancho.
function breakpoints(page: Page) {
  return page.locator('.cm-breakpoint').filter({ visible: true });
}

async function setSpeed(page: Page, index: number): Promise<void> {
  await page.locator('#speed').fill(String(index));
}

test('paso y paso atrás con botones y teclado', async ({ page }) => {
  await open(page);
  const status = page.locator('#run-status');
  await expect(page.locator('#step-back')).toBeDisabled();

  await page.locator('#step').click();
  await expect(status).toContainText('ciclo 1 ·');
  // `lw a0, n` son dos instrucciones: tras la primera sigue en su línea.
  await expect(page.locator('.cm-exec-line')).toContainText('lw    a0, n');

  await page.keyboard.press('F10');
  await page.keyboard.press('F10');
  await expect(status).toContainText('ciclo 3 ·');
  await expect(page.locator('.cm-exec-line')).toContainText('call  factorial');
  // `auipc ra` del `call`: ra acaba de cambiar.
  await expect(register(page, 'ra')).toHaveAttribute('data-changed', 'true');

  await page.keyboard.press('Shift+F10');
  await expect(status).toContainText('ciclo 2 ·');
  await page.locator('#step-back').click();
  await page.locator('#step-back').click();
  await expect(status).toContainText('ciclo 0 ·');
  await expect(page.locator('#step-back')).toBeDisabled();
});

test('ejecutar hasta el final y reiniciar', async ({ page }) => {
  await open(page);
  await setSpeed(page, 8);
  await page.locator('#run').click();
  await expect(page.locator('#run-status')).toContainText('terminado con código 0');
  await expect(register(page, 'a0').locator('.register-value')).toHaveText('0x000002d0');
  await expect(page.locator('#step')).toBeDisabled();
  await expect(page.locator('.cm-exec-line')).toHaveAttribute('data-kind', 'halted');

  await page.locator('#reset').click();
  await expect(page.locator('#run-status')).toContainText('listo');
  await expect(register(page, 'a0').locator('.register-value')).toHaveText('0x00000000');
});

test('puntos de parada desde el margen y con F9', async ({ page }) => {
  await open(page);
  await setSpeed(page, 8);
  const line = page.locator('.cm-line').nth(MUL_LINE - 1);
  const gutter = page.locator('.cm-breakpoint-gutter');
  const lineBox = await line.boundingBox();
  const gutterBox = await gutter.boundingBox();
  if (lineBox === null || gutterBox === null) {
    throw new Error('El editor no está en pantalla');
  }
  await page.mouse.click(gutterBox.x + gutterBox.width / 2, lineBox.y + lineBox.height / 2);
  await expect(breakpoints(page)).toHaveCount(1);

  const status = page.locator('#run-status');
  await page.keyboard.press('Control+Enter');
  await expect(status).toContainText(`punto de parada · línea ${String(MUL_LINE)}`);
  await expect(page.locator('.cm-exec-line')).toContainText('mul');
  // t1 = 1 en la vuelta más profunda; al reanudar para en la siguiente.
  await expect(register(page, 't1').locator('.register-value')).toHaveText('0x00000002');
  await page.locator('#run').click();
  await expect(status).toContainText(`punto de parada · línea ${String(MUL_LINE)}`);
  await expect(register(page, 't1').locator('.register-value')).toHaveText('0x00000003');

  // Quitarlo con F9 desde la línea y terminar.
  await line.click();
  await page.keyboard.press('F9');
  await expect(breakpoints(page)).toHaveCount(0);
  await page.keyboard.press('Control+Enter');
  await expect(status).toContainText('terminado con código 0');
});

test('pide entrada por la consola y sigue sola', async ({ page }) => {
  await open(page);
  await setSource(
    page,
    ['li a7, 5', 'ecall', 'addi a0, a0, 1', 'li a7, 1', 'ecall', 'li a7, 10', 'ecall'].join('\n'),
  );
  await setSpeed(page, 8);
  await page.locator('#run').click();
  await expect(page.locator('#run-status')).toContainText('esperando entrada');
  await expect(page.locator('#console-input')).toBeFocused();
  await page.locator('#console-input').fill('41');
  await page.keyboard.press('Enter');
  await expect(page.locator('#run-status')).toContainText('terminado con código 0');
  await expect(page.locator('#console-output')).toHaveText('42');
});

test('con errores no deja ejecutar', async ({ page }) => {
  await open(page);
  await page.locator('.cm-content').fill('addi a0, a0\n');
  await expect(page.locator('#assembly-status')).toContainText('línea 1');
  await expect(page.locator('#run-status')).toHaveText('sin programa ensamblado');
  for (const id of ['#step', '#run', '#reset', '#step-back']) {
    await expect(page.locator(id)).toBeDisabled();
  }
  await page.keyboard.press('F10');
  await expect(page.locator('#run-status')).toHaveText('sin programa ensamblado');
});

test('editar durante la ejecución la para', async ({ page }) => {
  await open(page);
  await setSource(page, 'loop:\n  addi t0, t0, 1\n  j loop\n');
  await setSpeed(page, 3);
  await page.locator('#run').click();
  await expect(page.locator('#run')).toHaveText('Pausa');
  await page.locator('.cm-content').press('End');
  await page.keyboard.type(' ');
  await expect(page.locator('#run')).toHaveText('Ejecutar');
  await expect(page.locator('#run-status')).toContainText('listo');
});

function memoryCell(page: Page, address: number) {
  return page.locator(`.memory-cell[title="0x${address.toString(16).padStart(8, '0')}"]`);
}

async function format(page: Page, id: string): Promise<void> {
  await page.locator(`#register-format input[value="${id}"]`).check({ force: true });
}

test('registros en hexadecimal, decimal, sin signo y ASCII', async ({ page }) => {
  await open(page);
  await setSpeed(page, 8);
  await page.locator('#run').click();
  await expect(page.locator('#run-status')).toContainText('terminado con código 0');
  const a0 = register(page, 'a0').locator('.register-value');
  const sp = register(page, 'sp').locator('.register-value');
  await expect(a0).toHaveText('0x000002d0');

  await format(page, 'signed');
  await expect(a0).toHaveText('720');
  await format(page, 'unsigned');
  await expect(sp).toHaveText(String(0x7ffffff0));
  // 0x2d0 no tiene ningún byte imprimible; los casos con letras están en los tests unitarios.
  await format(page, 'ascii');
  await expect(a0).toHaveText('····');

  // El formato elegido se mantiene al seguir trabajando.
  await format(page, 'signed');
  await page.locator('#reset').click();
  await expect(a0).toHaveText('0');
  await expect(sp).toHaveText(String(0x7ffffff0 | 0));
});

test('la memoria sigue la pila y marca lo último escrito', async ({ page }) => {
  await open(page);
  await expect(page.locator('#memory-status')).toContainText('Pila');
  await expect(page.locator('#memory-follow-sp')).toBeChecked();
  // `lw a0, n` (2), `call` (2), `addi sp`, `sw ra, 4(sp)`.
  for (let i = 0; i < 6; i += 1) {
    await page.keyboard.press('F10');
  }
  await expect(page.locator('#run-status')).toContainText('ciclo 6 ·');
  const saved = memoryCell(page, 0x7fffffec);
  await expect(saved).toHaveText('0x00000010');
  await expect(saved).toHaveAttribute('data-access', 'store');
  await expect(memoryCell(page, 0x7fffffe8)).toHaveAttribute('data-sp', 'true');
  // La tabla se desplaza por dentro hasta la fila de sp.
  const visible = await saved.evaluate((cell) => {
    const scroller = cell.closest('.memory-scroll');
    if (scroller === null) {
      return false;
    }
    const outer = scroller.getBoundingClientRect();
    const inner = cell.getBoundingClientRect();
    return inner.top >= outer.top && inner.bottom <= outer.bottom;
  });
  expect(visible).toBe(true);

  // Un paso más: el resaltado pasa a `sw a0, 0(sp)`.
  await page.keyboard.press('F10');
  await expect(memoryCell(page, 0x7fffffe8)).toHaveAttribute('data-access', 'store');
  await expect(memoryCell(page, 0x7fffffe8)).toHaveText('0x00000006');
  await expect(saved).not.toHaveAttribute('data-access', 'store');

  // Paso atrás deshace también la memoria.
  await page.keyboard.press('Shift+F10');
  await page.keyboard.press('Shift+F10');
  await expect(saved).toHaveText('0x00000000');
});

test('ir a una etiqueta, cambiar de unidad y de página', async ({ page }) => {
  await open(page);
  const status = page.locator('#memory-status');
  const goto = page.locator('#memory-goto-input');

  await goto.fill('n');
  await goto.press('Enter');
  await expect(status).toHaveText('Datos · 0x10000000 – 0x100000ff');
  await expect(page.locator('#memory-follow-sp')).not.toBeChecked();
  await expect(memoryCell(page, 0x10000000)).toHaveText('0x00000006');
  await expect(memoryCell(page, 0x10000000)).toHaveAttribute('data-target', 'true');

  await page.locator('#memory-unit input[value="byte"]').check({ force: true });
  await expect(memoryCell(page, 0x10000000)).toHaveText('06');
  // `lw a0, n` lee los cuatro bytes en su segundo ciclo.
  await page.keyboard.press('F10');
  await page.keyboard.press('F10');
  await expect(page.locator('.memory-cell[data-access="load"]')).toHaveCount(4);

  await page.locator('#memory-next').click();
  await expect(status).toHaveText('Datos · 0x10000100 – 0x100001ff');
  await page.locator('#memory-grid').focus();
  await page.keyboard.press('PageUp');
  await expect(status).toHaveText('Datos · 0x10000000 – 0x100000ff');

  await goto.fill('sp - 8');
  await goto.press('Enter');
  await expect(status).toHaveText('Pila · 0x7fffff00 – 0x7fffffff');
  await goto.fill('bucle');
  await goto.press('Enter');
  await expect(status).toHaveText('no hay ninguna etiqueta ni registro «bucle»');

  await page.locator('#memory-region input[value="text"]').check({ force: true });
  await expect(status).toContainText('Texto · 0x00000000');
  await expect(memoryCell(page, 0x00000000)).toHaveText('17');
});
