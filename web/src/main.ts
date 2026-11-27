import './styles.css';
import factorialSource from '../../examples/factorial.s?raw';
import {
  loadCore,
  type AssembleReport,
  type Core,
  type Label,
  type Machine,
  type ModelInfo,
  type RunStop,
  type PipelineTable,
  type Snapshot,
} from './core';
import { CheatSheetPanel, type CheatSheetElements } from './cheatsheet/panel';
import { DatapathPanel, type DatapathElements } from './datapath/panel';
import { PipelineTablePanel, type PipelineTableElements } from './pipeline/panel';
import { createEditor, type SourceEditor } from './editor/editor';
import { MemoryPanel, RegisterPanel, radioGroup, type MemoryElements } from './inspect/panels';
import { breakpointAddresses, executionLocation, sourceMap, type SourceMap } from './run/location';
import { Runner, browserScheduler } from './run/runner';
import { DEFAULT_SPEED_INDEX, SPEEDS, speedAt } from './run/speed';
import { plural, runCounters } from './run/stats';
import { THEME_CHOICES, applyTheme, readTheme, storeTheme } from './theme/theme';

// Pantalla de trabajo: editor, controles de ejecución, registros y consola. Todo lo que pasa
// en el procesador lo dice el núcleo; aquí solo se le piden operaciones y se pinta la foto.

// Lo bastante corto para que parezca inmediato y lo bastante largo para no ensamblar con
// cada tecla.
const ASSEMBLE_DELAY_MS = 250;

function element<T extends HTMLElement>(selector: string, type: new () => T): T {
  const found = document.querySelector(selector);
  if (!(found instanceof type)) {
    throw new Error(`Falta el elemento ${selector} en index.html`);
  }
  return found;
}

// El modelo elegido se recuerda entre visitas; si el almacenamiento no está disponible (modo
// privado, cuota), simplemente se empieza por el de un ciclo.
const MODEL_KEY = 'rvscope.model';

function storedModel(models: readonly ModelInfo[]): string | null {
  try {
    const id = window.localStorage.getItem(MODEL_KEY);
    return models.some((model) => model.id === id) ? id : null;
  } catch (error: unknown) {
    console.warn('rvscope: no se pudo leer el modelo guardado', error);
    return null;
  }
}

function storeModel(id: string): void {
  try {
    window.localStorage.setItem(MODEL_KEY, id);
  } catch (error: unknown) {
    console.warn('rvscope: no se pudo guardar el modelo', error);
  }
}

function modelNote(model: ModelInfo): string {
  return model.equivalent
    ? model.title
    : `${model.title}: si una instrucción usa un registro que otra muy cercana aún no ha escrito, lee el valor viejo y el resultado puede no coincidir con el del procesador de un ciclo.`;
}

function describeError(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
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

// Frase de estado de la ejecución: dónde está y por qué se paró.
function describeRun(snapshot: Snapshot, running: boolean, breakpointLine: number | null): string {
  const counters = runCounters(snapshot).join(' · ');
  const halt = snapshot.halt;
  let state: string;
  if (running) {
    state = 'ejecutando';
  } else if (halt !== null) {
    const where = halt.line === null ? '' : ` (línea ${String(halt.line)})`;
    state =
      halt.kind === 'fault'
        ? `fallo${where}: ${halt.message}`
        : halt.kind === 'exited'
          ? `terminado con código ${String(halt.code ?? 0)}`
          : `parado por ebreak${where}`;
  } else if (snapshot.status === 'awaiting_input') {
    state = 'esperando entrada';
  } else if (breakpointLine !== null) {
    state = `punto de parada · línea ${String(breakpointLine)}`;
  } else {
    state = snapshot.stats.cycles === 0 ? 'listo' : 'en pausa';
  }
  return `${state} · ${counters}`;
}

interface Controls {
  readonly model: HTMLSelectElement;
  readonly modelNote: HTMLElement;
  readonly assemble: HTMLButtonElement;
  readonly reset: HTMLButtonElement;
  readonly stepBack: HTMLButtonElement;
  readonly step: HTMLButtonElement;
  readonly run: HTMLButtonElement;
  readonly speed: HTMLInputElement;
  readonly speedValue: HTMLOutputElement;
  readonly runStatus: HTMLElement;
  readonly registers: HTMLElement;
  readonly registerFormat: HTMLElement;
  readonly memory: MemoryElements;
  readonly consoleOutput: HTMLElement;
  readonly consoleForm: HTMLFormElement;
  readonly consoleInput: HTMLInputElement;
  readonly consoleClose: HTMLButtonElement;
  readonly assemblyStatus: HTMLElement;
  readonly datapath: DatapathElements;
  readonly pipeline: PipelineTableElements;
  readonly cheatSheet: CheatSheetElements;
}

function findControls(): Controls {
  return {
    model: element('#model', HTMLSelectElement),
    modelNote: element('#model-note', HTMLElement),
    assemble: element('#assemble', HTMLButtonElement),
    reset: element('#reset', HTMLButtonElement),
    stepBack: element('#step-back', HTMLButtonElement),
    step: element('#step', HTMLButtonElement),
    run: element('#run', HTMLButtonElement),
    speed: element('#speed', HTMLInputElement),
    speedValue: element('#speed-value', HTMLOutputElement),
    runStatus: element('#run-status', HTMLElement),
    registers: element('#registers', HTMLElement),
    registerFormat: element('#register-format', HTMLElement),
    memory: {
      regions: element('#memory-region', HTMLElement),
      units: element('#memory-unit', HTMLElement),
      gotoForm: element('#memory-goto', HTMLFormElement),
      gotoInput: element('#memory-goto-input', HTMLInputElement),
      gotoButton: element('#memory-goto-button', HTMLButtonElement),
      previous: element('#memory-previous', HTMLButtonElement),
      next: element('#memory-next', HTMLButtonElement),
      followSp: element('#memory-follow-sp', HTMLInputElement),
      status: element('#memory-status', HTMLElement),
      grid: element('#memory-grid', HTMLTableElement),
    },
    consoleOutput: element('#console-output', HTMLElement),
    consoleForm: element('#console-form', HTMLFormElement),
    consoleInput: element('#console-input', HTMLInputElement),
    consoleClose: element('#console-close', HTMLButtonElement),
    assemblyStatus: element('#assembly-status', HTMLElement),
    datapath: {
      section: element('#datapath', HTMLElement),
      figure: element('#datapath-figure', HTMLElement),
      caption: element('#datapath-caption', HTMLElement),
    },
    pipeline: {
      section: element('#pipeline', HTMLElement),
      scroller: element('#pipeline-scroll', HTMLElement),
      table: element('#pipeline-table', HTMLTableElement),
      caption: element('#pipeline-caption', HTMLElement),
    },
    cheatSheet: {
      toggle: element('#cheatsheet-toggle', HTMLButtonElement),
      drawer: element('#cheatsheet', HTMLElement),
      title: element('#cheatsheet-title', HTMLElement),
      close: element('#cheatsheet-close', HTMLButtonElement),
      nav: element('#cheatsheet-nav', HTMLElement),
      body: element('#cheatsheet-body', HTMLElement),
    },
  };
}

class Workbench {
  readonly #core: Core;
  readonly #machine: Machine;
  readonly #ui: Controls;
  readonly #editor: SourceEditor;
  readonly #runner: Runner;
  readonly #registers: RegisterPanel;
  readonly #memory: MemoryPanel;
  readonly #datapath: DatapathPanel;
  readonly #pipeline: PipelineTablePanel;
  readonly #cheatSheet: CheatSheetPanel;
  // Etiquetas del último ensamblado correcto, para «Ir a» en la memoria.
  #labels: readonly Label[] = [];
  // Listado del último ensamblado correcto; `null` si hay errores o se editó después.
  #map: SourceMap | null = null;
  #pendingAssembly: number | undefined;
  // Línea del punto de parada donde se detuvo la última ejecución, hasta el siguiente paso.
  #breakpointLine: number | null = null;
  // La ejecución se paró a pedir entrada: al darla, sigue sola.
  #resumeAfterInput = false;
  #lastOutput = '';

  constructor(core: Core) {
    this.#core = core;
    this.#machine = core.createMachine();
    this.#ui = findControls();
    this.#registers = new RegisterPanel(
      this.#ui.registers,
      this.#ui.registerFormat,
      core.registers,
    );
    this.#datapath = new DatapathPanel(this.#ui.datapath, core.registers);
    this.#pipeline = new PipelineTablePanel(this.#ui.pipeline, core.registers);
    this.#cheatSheet = new CheatSheetPanel(this.#ui.cheatSheet, core.cheatSheet);
    this.#memory = new MemoryPanel(this.#ui.memory, (error) => {
      this.#fail(error);
    });
    this.#setupModels();
    this.#ui.speed.max = String(SPEEDS.length - 1);
    this.#ui.speed.value = String(DEFAULT_SPEED_INDEX);
    this.#runner = new Runner(
      this.#machine,
      speedAt(DEFAULT_SPEED_INDEX),
      {
        onProgress: () => {
          this.#render();
        },
        onStop: (stop) => {
          this.#stopped(stop);
        },
        onError: (error) => {
          this.#fail(error);
        },
      },
      browserScheduler,
    );
    this.#editor = createEditor({
      parent: element('#editor', HTMLElement),
      listingParent: element('#listing', HTMLElement),
      vocabulary: core.vocabulary,
      source: factorialSource,
      onChange: () => {
        this.#sourceChanged();
      },
      onBreakpointsChange: () => {
        this.#syncBreakpoints();
      },
    });
    this.#bind();
    this.#showSpeed();
    this.#assemble();
  }

  // El enlace apunta al contenedor (funciona sin JS); el foco tiene que ir al área editable.
  focusEditor(): void {
    this.#editor.view.focus();
  }

  #setupModels(): void {
    const select = this.#ui.model;
    for (const model of this.#core.models) {
      const option = document.createElement('option');
      option.value = model.id;
      option.textContent = model.label;
      option.title = model.title;
      select.append(option);
    }
    const stored = storedModel(this.#core.models);
    if (stored !== null) {
      this.#machine.setModel(stored);
    }
    select.value = this.#machine.model;
    this.#showModel();
  }

  #currentModel(): ModelInfo | undefined {
    return this.#core.models.find((info) => info.id === this.#machine.model);
  }

  #showModel(): void {
    const model = this.#currentModel();
    if (model === undefined) {
      return;
    }
    this.#ui.model.title = model.title;
    this.#ui.modelNote.textContent = modelNote(model);
    this.#ui.modelNote.dataset.warning = String(!model.equivalent);
  }

  // Ruta de datos y tabla del pipeline. La tabla se pide al núcleo aparte y solo si el modelo
  // la enseña: con el de un ciclo el panel está oculto.
  #renderDatapath(snapshot: Snapshot | null): void {
    const model = this.#currentModel();
    if (model === undefined) {
      return;
    }
    this.#datapath.render(model, snapshot);
    if (snapshot === null || !model.pipelined) {
      this.#pipeline.render(model, null);
      return;
    }
    let table: PipelineTable;
    try {
      table = this.#machine.pipelineTable();
    } catch (error: unknown) {
      this.#fail(error);
      return;
    }
    this.#pipeline.render(model, table);
  }

  // Cambiar de procesador empieza de cero con el mismo programa; los puntos de parada viven en
  // la sesión y se conservan.
  #changeModel(id: string): void {
    this.#runner.stop();
    this.#resumeAfterInput = false;
    this.#breakpointLine = null;
    try {
      this.#machine.setModel(id);
    } catch (error: unknown) {
      this.#ui.model.value = this.#machine.model;
      this.#fail(error);
      return;
    }
    storeModel(id);
    this.#showModel();
    this.#render();
  }

  #bind(): void {
    const ui = this.#ui;
    ui.model.addEventListener('change', () => {
      this.#changeModel(ui.model.value);
    });
    ui.assemble.addEventListener('click', () => {
      this.#assemble();
    });
    ui.reset.addEventListener('click', () => {
      this.#act(() => {
        this.#machine.reset();
      });
    });
    ui.stepBack.addEventListener('click', () => {
      this.#stepBack();
    });
    ui.step.addEventListener('click', () => {
      this.#step();
    });
    ui.run.addEventListener('click', () => {
      this.#toggleRun();
    });
    ui.speed.addEventListener('input', () => {
      this.#showSpeed();
    });
    ui.consoleForm.addEventListener('submit', (event) => {
      event.preventDefault();
      const text = ui.consoleInput.value;
      ui.consoleInput.value = '';
      this.#giveInput(() => {
        this.#machine.pushInput(`${text}\n`);
      });
    });
    ui.consoleClose.addEventListener('click', () => {
      this.#giveInput(() => {
        this.#machine.closeInput();
      });
    });
    // En captura: los atajos funcionan también con el foco dentro del editor.
    window.addEventListener(
      'keydown',
      (event) => {
        this.#shortcut(event);
      },
      { capture: true },
    );
  }

  #shortcut(event: KeyboardEvent): void {
    const mod = event.ctrlKey || event.metaKey;
    let action: (() => void) | null = null;
    if (event.key === 'F9' && !mod && !event.altKey) {
      action = () => {
        this.#editor.toggleBreakpointAtCursor();
      };
    } else if (event.key === 'F10' && !mod && !event.altKey) {
      action = event.shiftKey
        ? () => {
            this.#stepBack();
          }
        : () => {
            this.#step();
          };
    } else if (event.key === 'F2' && !mod && !event.altKey && !event.shiftKey) {
      action = () => {
        this.#cheatSheet.toggle();
      };
    } else if (event.key === 'Enter' && mod && !event.shiftKey && !event.altKey) {
      action = () => {
        this.#toggleRun();
      };
    } else if (event.key.toLowerCase() === 's' && mod && !event.shiftKey && !event.altKey) {
      action = () => {
        this.#assemble();
      };
    }
    if (action !== null) {
      event.preventDefault();
      event.stopPropagation();
      action();
    }
  }

  #showSpeed(): void {
    const speed = speedAt(Number(this.#ui.speed.value));
    this.#ui.speedValue.textContent = speed.label;
    this.#ui.speed.setAttribute('aria-valuetext', speed.label);
    this.#runner.setSpeed(speed);
  }

  #sourceChanged(): void {
    // El programa en marcha ya no es el del editor.
    this.#runner.stop();
    this.#resumeAfterInput = false;
    this.#map = null;
    this.#render();
    window.clearTimeout(this.#pendingAssembly);
    this.#pendingAssembly = window.setTimeout(() => {
      this.#assemble();
    }, ASSEMBLE_DELAY_MS);
  }

  #assemble(): void {
    window.clearTimeout(this.#pendingAssembly);
    this.#runner.stop();
    this.#resumeAfterInput = false;
    this.#breakpointLine = null;
    try {
      const report = this.#machine.assemble(this.#editor.source());
      this.#editor.showReport(report);
      renderAssembly(report, this.#ui.assemblyStatus);
      this.#map = report.ok ? sourceMap(report.listing) : null;
      this.#labels = report.labels;
      if (report.ok) {
        this.#memory.load(this.#machine);
      }
      this.#syncBreakpoints();
    } catch (error: unknown) {
      this.#map = null;
      this.#fail(error);
    }
    this.#render();
  }

  #syncBreakpoints(): void {
    if (this.#map === null) {
      return;
    }
    this.#machine.setBreakpoints(breakpointAddresses(this.#editor.breakpointLines(), this.#map));
  }

  #step(): void {
    this.#act(() => {
      if (this.#machine.step() === 'awaiting_input') {
        this.#ui.consoleInput.focus();
      }
    });
  }

  #stepBack(): void {
    this.#act(() => {
      this.#machine.stepBack();
    });
  }

  #toggleRun(): void {
    if (this.#runner.running) {
      this.#runner.stop();
      this.#resumeAfterInput = false;
      this.#render();
      return;
    }
    if (!this.#ready()) {
      return;
    }
    const snapshot = this.#machine.snapshot();
    if (snapshot.status === 'halted') {
      return;
    }
    if (snapshot.status === 'awaiting_input') {
      this.#resumeAfterInput = true;
      this.#ui.consoleInput.focus();
      this.#render();
      return;
    }
    this.#breakpointLine = null;
    this.#runner.start();
    this.#render();
  }

  #stopped(stop: Exclude<RunStop, { reason: 'budget' }>): void {
    if (stop.reason === 'breakpoint') {
      this.#breakpointLine = this.#map?.lineOfAddress.get(stop.pc) ?? null;
    } else if (stop.reason === 'awaiting_input') {
      this.#resumeAfterInput = true;
      this.#ui.consoleInput.focus();
    }
    this.#render();
  }

  #giveInput(give: () => void): void {
    if (!this.#ready()) {
      return;
    }
    // `#act` olvida la reanudación pendiente: se guarda antes.
    const resume = this.#resumeAfterInput;
    this.#act(give);
    if (resume && this.#machine.snapshot().status === 'ready') {
      this.#runner.start();
      this.#render();
    }
  }

  // Un paso suelto: para la ejecución continua, hace la operación y repinta.
  #act(operation: () => void): void {
    if (!this.#ready()) {
      return;
    }
    this.#runner.stop();
    this.#resumeAfterInput = false;
    this.#breakpointLine = null;
    try {
      operation();
    } catch (error: unknown) {
      this.#fail(error);
      return;
    }
    this.#render();
  }

  #ready(): boolean {
    return this.#map !== null;
  }

  #fail(error: unknown): void {
    this.#runner.stop();
    this.#ui.runStatus.dataset.state = 'error';
    this.#ui.runStatus.textContent = `error del simulador: ${describeError(error)}`;
    console.error('rvscope:', error);
  }

  #render(): void {
    const ui = this.#ui;
    const ready = this.#ready();
    const running = this.#runner.running;
    ui.run.textContent = running ? 'Pausa' : 'Ejecutar';
    ui.run.setAttribute('aria-pressed', String(running));
    if (!ready) {
      for (const button of [ui.reset, ui.stepBack, ui.step, ui.run]) {
        button.disabled = true;
      }
      ui.consoleInput.disabled = true;
      ui.consoleClose.disabled = true;
      ui.runStatus.dataset.state = 'idle';
      ui.runStatus.textContent = 'sin programa ensamblado';
      this.#editor.showLocation(null);
      this.#registers.render(null);
      this.#memory.clear();
      this.#renderDatapath(null);
      return;
    }
    let snapshot: Snapshot;
    try {
      snapshot = this.#machine.snapshot();
    } catch (error: unknown) {
      this.#fail(error);
      return;
    }
    const halted = snapshot.status === 'halted';
    ui.reset.disabled = false;
    ui.stepBack.disabled = !snapshot.can_step_back;
    ui.step.disabled = halted;
    ui.run.disabled = halted && !running;
    const acceptsInput = !halted && !snapshot.input_closed;
    ui.consoleInput.disabled = !acceptsInput;
    ui.consoleClose.disabled = !acceptsInput;
    ui.consoleForm.dataset.awaiting = String(snapshot.status === 'awaiting_input');

    ui.runStatus.dataset.state =
      snapshot.halt?.kind === 'fault' ? 'error' : running ? 'running' : 'idle';
    ui.runStatus.textContent = describeRun(snapshot, running, this.#breakpointLine);

    this.#registers.render(snapshot);
    this.#renderDatapath(snapshot);
    this.#memory.render(snapshot, {
      labels: this.#labels,
      registers: this.#core.registers,
      values: snapshot.registers,
    });
    if (snapshot.output !== this.#lastOutput) {
      ui.consoleOutput.textContent = snapshot.output;
      ui.consoleOutput.scrollTop = ui.consoleOutput.scrollHeight;
      this.#lastOutput = snapshot.output;
    }
    if (this.#map !== null) {
      this.#editor.showLocation(executionLocation(snapshot, this.#map));
    }
  }
}

function setupTheme(): void {
  const root = document.documentElement;
  const storage = (): Storage => window.localStorage;
  radioGroup(
    element('#theme', HTMLFieldSetElement),
    'theme',
    THEME_CHOICES,
    readTheme(storage),
    (choice) => {
      applyTheme(root, choice);
      storeTheme(storage, choice);
    },
  );
}

async function boot(): Promise<void> {
  setupTheme();
  const status = element('#core-status', HTMLElement);
  try {
    const core = await loadCore();
    status.textContent = `núcleo ${core.version}`;
    status.dataset.state = 'ready';
    const workbench = new Workbench(core);
    element('.skip-link', HTMLAnchorElement).addEventListener('click', (event) => {
      event.preventDefault();
      workbench.focusEditor();
    });
  } catch (error: unknown) {
    status.textContent = 'no se pudo cargar el núcleo';
    status.dataset.state = 'error';
    console.error('rvscope:', describeError(error));
  }
}

void boot();
