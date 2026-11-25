import init, { version } from './wasm/pkg/rv_wasm.js';

export interface CoreInfo {
  readonly version: string;
}

// El wasm se inicializa una sola vez; las llamadas posteriores reutilizan la misma promesa.
let ready: Promise<CoreInfo> | null = null;

export function loadCore(): Promise<CoreInfo> {
  ready ??= init().then(() => ({ version: version() }));
  return ready;
}
