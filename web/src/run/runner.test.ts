import { describe, expect, it } from 'vitest';
import type { RunStop } from '../core';
import { Runner, type FrameScheduler } from './runner';

// Fotogramas a mano: cada `tick` avanza el reloj y dispara el fotograma pendiente.
class ManualFrames implements FrameScheduler {
  clock = 0;
  #pending: ((now: number) => void) | null = null;

  request(callback: (now: number) => void): number {
    this.#pending = callback;
    return 1;
  }

  cancel(): void {
    this.#pending = null;
  }

  now(): number {
    return this.clock;
  }

  get pending(): boolean {
    return this.#pending !== null;
  }

  tick(ms: number): void {
    this.clock += ms;
    const callback = this.#pending;
    this.#pending = null;
    callback?.(this.clock);
  }
}

// Programa que se para al llegar a `length` ciclos.
class Counter {
  cycles = 0;
  calls: number[] = [];

  constructor(readonly length: number) {}

  run(max: number): RunStop {
    this.calls.push(max);
    const done = Math.min(max, this.length - this.cycles);
    this.cycles += done;
    return this.cycles >= this.length ? { reason: 'halted' } : { reason: 'budget' };
  }
}

function setup(length: number, cyclesPerSecond: number | null) {
  const frames = new ManualFrames();
  const target = new Counter(length);
  const stops: RunStop[] = [];
  let progress = 0;
  const runner = new Runner(
    target,
    { label: 'x', cyclesPerSecond },
    {
      onProgress: () => {
        progress += 1;
      },
      onStop: (stop) => stops.push(stop),
      onError: (error) => {
        throw error;
      },
    },
    frames,
  );
  return { frames, target, stops, runner, progress: () => progress };
}

describe('Runner', () => {
  it('ejecuta a la velocidad pedida y avisa al pararse', () => {
    const { frames, target, stops, runner } = setup(3, 2);
    runner.start();
    frames.tick(16);
    expect(target.cycles).toBe(1);
    for (let frame = 0; frame < 60; frame += 1) {
      frames.tick(1000 / 60);
    }
    expect(target.cycles).toBe(3);
    expect(stops).toEqual([{ reason: 'halted' }]);
    expect(runner.running).toBe(false);
    expect(frames.pending).toBe(false);
  });

  it('se detiene con stop sin avisar', () => {
    const { frames, target, stops, runner } = setup(1000, 10);
    runner.start();
    frames.tick(16);
    runner.stop();
    frames.tick(1000);
    expect(target.cycles).toBe(1);
    expect(stops).toEqual([]);
    expect(runner.running).toBe(false);
  });

  it('solo repinta en los fotogramas que ejecutan algo', () => {
    const { frames, runner, progress } = setup(1000, 1);
    runner.start();
    frames.tick(16);
    frames.tick(16);
    frames.tick(16);
    expect(progress()).toBe(1);
  });

  it('avisa de un error del núcleo y se para', () => {
    const frames = new ManualFrames();
    const errors: unknown[] = [];
    const runner = new Runner(
      {
        run: () => {
          throw new Error('sin programa');
        },
      },
      { label: 'x', cyclesPerSecond: 10 },
      { onProgress: () => undefined, onStop: () => undefined, onError: (e) => errors.push(e) },
      frames,
    );
    runner.start();
    frames.tick(16);
    expect(errors).toHaveLength(1);
    expect(runner.running).toBe(false);
  });

  it('a velocidad máxima pide tramos grandes', () => {
    const { frames, target, stops, runner } = setup(20_000, null);
    runner.start();
    for (let frame = 0; frame < 10 && runner.running; frame += 1) {
      frames.tick(16);
    }
    expect(target.calls.length).toBeGreaterThan(1);
    expect(stops).toEqual([{ reason: 'halted' }]);
  });
});
