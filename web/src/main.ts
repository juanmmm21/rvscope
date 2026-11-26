import './styles.css';
import factorialSource from '../../examples/factorial.s?raw';
import { loadCore, type AssembleReport, type Core, type Machine, type Snapshot } from './core';
import { createEditor } from './editor/editor';

// Pantalla de trabajo: editor con el factorial de ejemplo, ensamblado al dejar de escribir y
// registros del estado inicial. Los controles de ejecución llegan en la siguiente casilla.

// Lo bastante corto para que parezca inmediato y lo bastante largo para no ensamblar con
// cada tecla.
const ASSEMBLE_DELAY_MS = 250;

function element(selector: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(selector);
  if (found === null) {
    throw new Error(`Falta el elemento ${selector} en index.html`);
  }
  return found;
}

function hex(value: number): string {
  return `0x${(value >>> 0).toString(16).padStart(8, '0')}`;
}

function plural(count: number, one: string, many: string): string {
  return `${String(count)} ${count === 1 ? one : many}`;
}

function renderRegisters(core: Core, snapshot: Snapshot, target: HTMLElement): void {
  const rows = core.registers.map((info) => {
    const row = document.createElement('div');
    row.className = 'register';
    if (snapshot.changed.includes(info.index)) {
      row.dataset.changed = 'true';
    }
    const name = document.createElement('span');
    name.className = 'register-name';
    name.textContent = info.abi;
    name.title = `${info.name} · ${info.role}`;
    const value = document.createElement('span');
    value.className = 'register-value';
    value.textContent = hex(snapshot.registers[info.index] ?? 0);
    row.append(name, value);
    return row;
  });
  target.replaceChildren(...rows);
}

function renderAssembly(report: AssembleReport, target: HTMLElement): void {
  if (report.ok) {
    const bytes = report.listing.length * 4;
    target.dataset.state = 'ok';
    target.textContent = `ensamblado · ${plural(report.listing.length, 'instrucción', 'instrucciones')} · ${plural(bytes, 'byte', 'bytes')}`;
    return;
  }
  const [first] = report.errors;
  target.dataset.state = 'error';
  target.textContent =
    first === undefined
      ? 'no se pudo ensamblar'
      : `${plural(report.errors.length, 'error', 'errores')} · línea ${String(first.line)}, columna ${String(first.column)}: ${first.message}`;
}

function assembleAndShow(
  core: Core,
  machine: Machine,
  source: string,
  show: (report: AssembleReport) => void,
): void {
  const report = machine.assemble(source);
  show(report);
  renderAssembly(report, element('#assembly-status'));
  if (report.ok) {
    renderRegisters(core, machine.snapshot(), element('#registers'));
  }
}

async function boot(): Promise<void> {
  const status = element('#core-status');
  try {
    const core = await loadCore();
    status.textContent = `núcleo ${core.version}`;
    status.dataset.state = 'ready';

    const machine = core.createMachine();
    let pending: number | undefined;
    const editor = createEditor({
      parent: element('#editor'),
      listingParent: element('#listing'),
      vocabulary: core.vocabulary,
      source: factorialSource,
      onChange: () => {
        window.clearTimeout(pending);
        pending = window.setTimeout(() => {
          assembleAndShow(core, machine, editor.source(), (report) => {
            editor.showReport(report);
          });
        }, ASSEMBLE_DELAY_MS);
      },
    });
    assembleAndShow(core, machine, editor.source(), (report) => {
      editor.showReport(report);
    });
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    status.textContent = 'no se pudo cargar el núcleo';
    status.dataset.state = 'error';
    console.error('rvscope:', reason);
  }
}

void boot();
