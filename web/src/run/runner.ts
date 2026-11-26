import type { RunStop } from '../core';
import { Pacer, type Speed } from './speed';

// Ejecución continua a trozos, un trozo por fotograma, para que la página siga respondiendo
// mientras corre un programa largo. El reloj y los fotogramas se inyectan para probarlo sin
// navegador.

export interface RunTarget {
  run(maxCycles: number): RunStop;
}

export interface FrameScheduler {
  request(callback: (now: number) => void): number;
  cancel(handle: number): void;
  now(): number;
}

export const browserScheduler: FrameScheduler = {
  request: (callback) => window.requestAnimationFrame(callback),
  cancel: (handle) => {
    window.cancelAnimationFrame(handle);
  },
  now: () => performance.now(),
};

export interface RunnerEvents {
  // Tras cada fotograma en que se ejecutó algo: toca repintar.
  readonly onProgress: () => void;
  // La ejecución terminó sola (no por `stop`). `budget` nunca llega aquí.
  readonly onStop: (stop: Exclude<RunStop, { reason: 'budget' }>) => void;
  // El núcleo lanzó un error: la ejecución queda parada.
  readonly onError: (error: unknown) => void;
}

export class Runner {
  readonly #target: RunTarget;
  readonly #scheduler: FrameScheduler;
  readonly #events: RunnerEvents;
  readonly #pacer: Pacer;
  #handle: number | null = null;
  #last = 0;

  constructor(target: RunTarget, speed: Speed, events: RunnerEvents, scheduler: FrameScheduler) {
    this.#target = target;
    this.#scheduler = scheduler;
    this.#events = events;
    this.#pacer = new Pacer(speed);
  }

  get running(): boolean {
    return this.#handle !== null;
  }

  setSpeed(speed: Speed): void {
    this.#pacer.setSpeed(speed);
  }

  start(): void {
    if (this.#handle !== null) {
      return;
    }
    this.#pacer.restart();
    this.#last = this.#scheduler.now();
    this.#schedule();
  }

  stop(): void {
    if (this.#handle !== null) {
      this.#scheduler.cancel(this.#handle);
      this.#handle = null;
    }
  }

  #schedule(): void {
    this.#handle = this.#scheduler.request((now) => {
      this.#frame(now);
    });
  }

  #frame(now: number): void {
    const cycles = this.#pacer.take(now - this.#last);
    this.#last = now;
    if (cycles === 0) {
      this.#schedule();
      return;
    }
    const started = this.#scheduler.now();
    let stop: RunStop;
    try {
      stop = this.#target.run(cycles);
    } catch (error: unknown) {
      this.#handle = null;
      this.#events.onError(error);
      return;
    }
    this.#pacer.report(cycles, this.#scheduler.now() - started);
    if (stop.reason === 'budget') {
      this.#schedule();
      this.#events.onProgress();
      return;
    }
    this.#handle = null;
    this.#events.onProgress();
    this.#events.onStop(stop);
  }
}
