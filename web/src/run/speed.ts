// Velocidades de ejecución y el reparto de ciclos entre fotogramas. Sin DOM: se prueba aparte.

export interface Speed {
  readonly label: string;
  // `null` = tan rápido como permita el navegador sin congelar la página.
  readonly cyclesPerSecond: number | null;
}

export const SPEEDS: readonly Speed[] = [
  { label: '1 ciclo/s', cyclesPerSecond: 1 },
  { label: '2 ciclos/s', cyclesPerSecond: 2 },
  { label: '5 ciclos/s', cyclesPerSecond: 5 },
  { label: '10 ciclos/s', cyclesPerSecond: 10 },
  { label: '50 ciclos/s', cyclesPerSecond: 50 },
  { label: '250 ciclos/s', cyclesPerSecond: 250 },
  { label: '1000 ciclos/s', cyclesPerSecond: 1000 },
  { label: '10 000 ciclos/s', cyclesPerSecond: 10_000 },
  { label: 'máxima', cyclesPerSecond: null },
];

export const DEFAULT_SPEED_INDEX = 3;

// Tiempo de cálculo por fotograma a velocidad máxima: deja margen hasta los 16 ms de un
// fotograma a 60 Hz para pintar y atender al teclado.
const FRAME_BUDGET_MS = 12;
const MIN_CHUNK = 100;
const MAX_CHUNK = 2_000_000;
const INITIAL_CHUNK = 5_000;
// Tras una pausa larga (pestaña en segundo plano) no se recupera el tiempo perdido de golpe.
const MAX_CATCH_UP_SECONDS = 0.25;

export function speedAt(index: number): Speed {
  const speed = SPEEDS[Math.min(Math.max(Math.trunc(index), 0), SPEEDS.length - 1)];
  if (speed === undefined) {
    throw new Error('La tabla de velocidades está vacía');
  }
  return speed;
}

// Cuántos ciclos pedir en cada fotograma. Con velocidad fija acumula fracciones de ciclo; a
// velocidad máxima ajusta el tramo al tiempo que tardó el anterior.
export class Pacer {
  #speed: Speed;
  #carry = 0;
  #chunk = INITIAL_CHUNK;

  constructor(speed: Speed) {
    this.#speed = speed;
    this.restart();
  }

  get speed(): Speed {
    return this.#speed;
  }

  setSpeed(speed: Speed): void {
    this.#speed = speed;
    this.#carry = Math.min(this.#carry, 1);
  }

  // Al arrancar, el primer ciclo sale ya en el primer fotograma: a 1 ciclo/s no se espera un
  // segundo entero a ver algo.
  restart(): void {
    this.#carry = 1;
  }

  take(elapsedMs: number): number {
    const rate = this.#speed.cyclesPerSecond;
    if (rate === null) {
      return this.#chunk;
    }
    const elapsed = Math.max(elapsedMs, 0) / 1000;
    this.#carry = Math.min(this.#carry + rate * elapsed, Math.max(rate * MAX_CATCH_UP_SECONDS, 1));
    // El margen evita perder un ciclo por redondeo al sumar fracciones como 1/60.
    const cycles = Math.floor(this.#carry + 1e-9);
    this.#carry = Math.max(this.#carry - cycles, 0);
    return cycles;
  }

  // Solo cuenta a velocidad máxima: el tramo siguiente se escala para gastar FRAME_BUDGET_MS.
  report(cycles: number, spentMs: number): void {
    if (this.#speed.cyclesPerSecond !== null || cycles < this.#chunk) {
      return;
    }
    const scale = FRAME_BUDGET_MS / Math.max(spentMs, 0.5);
    // Se cambia como mucho al doble o a la mitad por fotograma para no oscilar.
    const next = this.#chunk * Math.min(Math.max(scale, 0.5), 2);
    this.#chunk = Math.round(Math.min(Math.max(next, MIN_CHUNK), MAX_CHUNK));
  }
}
