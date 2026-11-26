import './styles.css';
import factorialSource from '../../examples/factorial.s?raw';
import { loadCore, type Core, type Snapshot } from './core';

// Pantalla provisional de la fachada: ensambla el factorial de ejemplo, lo ejecuta y pinta
// registros y estado. El editor y los controles de verdad llegan en las siguientes casillas.

const RUN_BUDGET = 1_000_000;

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

function renderSummary(snapshot: Snapshot, model: string, target: HTMLElement): void {
  const cpi = snapshot.stats.cpi === null ? '—' : snapshot.stats.cpi.toFixed(2);
  const ending = snapshot.halt === null ? 'sin terminar' : snapshot.halt.message;
  const lines = [
    model,
    `${String(snapshot.stats.cycles)} ciclos · ${String(snapshot.stats.retired)} instrucciones · CPI ${cpi}`,
    ending,
  ];
  target.replaceChildren(
    ...lines.map((text) => {
      const line = document.createElement('p');
      line.textContent = text;
      return line;
    }),
  );
}

async function boot(): Promise<void> {
  const status = element('#core-status');
  try {
    const core = await loadCore();
    status.textContent = `núcleo ${core.version}`;
    status.dataset.state = 'ready';

    const machine = core.createMachine();
    const report = machine.assemble(factorialSource);
    if (!report.ok) {
      const first = report.errors[0];
      throw new Error(
        first === undefined
          ? 'el ejemplo no ensambla'
          : `línea ${String(first.line)}, columna ${String(first.column)}: ${first.message}`,
      );
    }
    machine.run(RUN_BUDGET);
    const snapshot = machine.snapshot();
    const model = core.models.find((info) => info.id === snapshot.model)?.title ?? snapshot.model;
    renderSummary(snapshot, model, element('#summary'));
    renderRegisters(core, snapshot, element('#registers'));
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    status.textContent = 'no se pudo cargar el núcleo';
    status.dataset.state = 'error';
    console.error('rvscope:', reason);
  }
}

void boot();
