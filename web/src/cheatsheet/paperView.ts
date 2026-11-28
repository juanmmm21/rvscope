import type { Paper, PaperBlock, PaperFormat } from './paper';

// Pinta la hoja imprimible dentro de `container`. En pantalla no se ve (la esconde la hoja de
// estilos fuera de `@media print`); al imprimir es lo único que sale. Se construye una vez.
export function renderPaper(container: HTMLElement, paper: Paper): void {
  const title = document.createElement('h2');
  title.className = 'paper-title';
  title.textContent = 'rvscope · chuleta de RISC-V · RV32IM';
  container.replaceChildren(
    title,
    formatTable(paper.formats),
    ...paper.blocks.map((block) => blockElement(block)),
  );
}

// Los seis formatos en una sola tabla de 32 columnas, una por bit, para que los campos de todos
// los formatos queden alineados entre sí como en las hojas de referencia de clase.
function formatTable(formats: readonly PaperFormat[]): HTMLElement {
  const table = document.createElement('table');
  table.className = 'paper-formats';
  const caption = document.createElement('caption');
  caption.textContent = 'Formatos (bit 31 a la izquierda)';
  table.append(caption);
  const body = table.createTBody();
  for (const format of formats) {
    const row = body.insertRow();
    const head = document.createElement('th');
    head.scope = 'row';
    head.textContent = format.format;
    row.append(head);
    for (const field of format.fields) {
      const cell = row.insertCell();
      cell.colSpan = field.width;
      const name = document.createElement('span');
      name.textContent = field.name;
      const range = document.createElement('small');
      range.textContent = field.range;
      cell.append(name, range);
    }
  }
  return table;
}

function blockElement(block: PaperBlock): HTMLElement {
  const section = document.createElement('section');
  section.className = 'paper-block';
  section.dataset.kind = block.kind;
  section.setAttribute('aria-labelledby', block.id);
  const heading = document.createElement('h3');
  heading.id = block.id;
  heading.textContent = block.title;
  const list = document.createElement('dl');
  for (const row of block.rows) {
    const entry = document.createElement('div');
    const term = document.createElement('dt');
    const syntax = document.createElement('code');
    if (row.name !== '' && row.syntax.startsWith(row.name)) {
      const mnemonic = document.createElement('b');
      mnemonic.textContent = row.name;
      syntax.append(mnemonic, row.syntax.slice(row.name.length));
    } else {
      syntax.textContent = row.syntax;
    }
    term.append(syntax);
    if (row.format !== null) {
      const tag = document.createElement('small');
      tag.textContent = row.format;
      term.append(' ', tag);
    }
    const detail = document.createElement('dd');
    detail.textContent = row.detail;
    entry.append(term, detail);
    list.append(entry);
  }
  section.append(heading, list);
  return section;
}
