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

// Páginas de un PDF: cada página es un objeto `/Type /Page` (los `/Pages` son el árbol).
function pdfPageCount(pdf: Buffer): number {
  return pdf.toString('latin1').match(/\/Type\s*\/Page(?!s)/gu)?.length ?? 0;
}

test('la hoja de papel no se ve en pantalla', async ({ page }) => {
  await open(page);
  await page.getByRole('button', { name: 'Chuleta' }).click();
  await expect(page.locator('#cheatsheet-paper')).toBeHidden();
  await expect(page.locator('#cheatsheet-print')).toBeVisible();
});

test('el botón imprimir pide imprimir al navegador', async ({ page }) => {
  await page.addInitScript(() => {
    const counter = window as unknown as { printCalls: number };
    counter.printCalls = 0;
    window.print = () => {
      counter.printCalls += 1;
    };
  });
  await open(page);
  await page.keyboard.press('F2');
  await page.locator('#cheatsheet-print').press('Enter');
  const calls = await page.evaluate(() => (window as unknown as { printCalls: number }).printCalls);
  expect(calls).toBe(1);
});

test('al imprimir solo sale la chuleta, con todo su contenido', async ({ page }) => {
  await open(page);
  await page.emulateMedia({ media: 'print' });
  const paper = page.locator('#cheatsheet-paper');
  await expect(paper).toBeVisible();
  for (const selector of ['.topbar', '.controls', 'main', '#cheatsheet']) {
    await expect(page.locator(selector)).toBeHidden();
  }
  await expect(paper.locator('.paper-formats tbody tr')).toHaveCount(6);
  // 48 instrucciones (cada una una vez), 36 pseudo y 23 directivas, como en el panel.
  await expect(paper.locator('.paper-block dt small')).toHaveCount(48);
  await expect(paper.locator('#paper-cs-pseudo ~ dl > div')).toHaveCount(36);
  await expect(paper.locator('#paper-cs-directives ~ dl > div')).toHaveCount(23);
  await expect(paper.locator('#paper-cs-registers ~ dl')).toContainText('x18–x27 s2–s11');
  await expect(paper.locator('#paper-cs-registers ~ dl')).toContainText('x2 sp');
  // Formato I de libro: el de `addi`, con el inmediato entero.
  await expect(paper.locator('.paper-formats tbody tr').nth(1)).toContainText('imm[11:0]');
});

test('la chuleta cabe en una sola hoja A4, también en modo oscuro', async ({ page }) => {
  await page.emulateMedia({ colorScheme: 'dark' });
  await open(page);
  const pdf = await page.pdf({ format: 'A4', preferCSSPageSize: true, printBackground: true });
  expect(pdfPageCount(pdf)).toBe(1);
  await page.emulateMedia({ media: 'print', colorScheme: 'dark' });
  const colors = await page
    .locator('#cheatsheet-paper')
    .evaluate((element) => [
      getComputedStyle(element).color,
      getComputedStyle(document.body).backgroundColor,
    ]);
  expect(colors).toEqual(['rgb(0, 0, 0)', 'rgb(255, 255, 255)']);
});
