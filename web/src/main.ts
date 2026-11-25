import './styles.css';
import { loadCore } from './core';

const status = document.querySelector<HTMLElement>('#core-status');

async function boot(): Promise<void> {
  if (status === null) {
    throw new Error('Falta el elemento #core-status en index.html');
  }
  try {
    const core = await loadCore();
    status.textContent = `núcleo ${core.version}`;
    status.dataset.state = 'ready';
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    status.textContent = 'no se pudo cargar el núcleo';
    status.dataset.state = 'error';
    console.error('rvscope: fallo al cargar el wasm', reason);
  }
}

void boot();
