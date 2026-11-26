import { describe, expect, it } from 'vitest';
import { Pacer, SPEEDS, speedAt } from './speed';

const rated = (cyclesPerSecond: number) => ({ label: 'x', cyclesPerSecond });
const max = { label: 'máxima', cyclesPerSecond: null };

describe('Pacer', () => {
  it('da el primer ciclo en el primer fotograma', () => {
    const pacer = new Pacer(rated(1));
    expect(pacer.take(0)).toBe(1);
    expect(pacer.take(16)).toBe(0);
  });

  it('acumula fracciones de ciclo entre fotogramas', () => {
    const pacer = new Pacer(rated(10));
    pacer.take(0);
    let total = 0;
    for (let frame = 0; frame < 60; frame += 1) {
      total += pacer.take(1000 / 60);
    }
    expect(total).toBe(10);
  });

  it('no recupera de golpe una pausa larga', () => {
    const pacer = new Pacer(rated(1000));
    pacer.take(0);
    expect(pacer.take(10_000)).toBe(250);
  });

  it('a velocidad máxima ajusta el tramo al tiempo gastado', () => {
    const pacer = new Pacer(max);
    const first = pacer.take(16);
    pacer.report(first, 3);
    expect(pacer.take(16)).toBe(first * 2);
    pacer.report(first * 2, 48);
    expect(pacer.take(16)).toBe(first);
  });

  it('no agranda el tramo si el programa paró antes de gastarlo', () => {
    const pacer = new Pacer(max);
    const first = pacer.take(16);
    pacer.report(10, 0.01);
    expect(pacer.take(16)).toBe(first);
  });
});

describe('speedAt', () => {
  it('se queda dentro de la tabla', () => {
    expect(speedAt(-3)).toBe(SPEEDS[0]);
    expect(speedAt(99)).toBe(SPEEDS[SPEEDS.length - 1]);
  });
});
