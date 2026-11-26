import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    target: 'es2022',
  },
  server: {
    // Los ejemplos viven en ../examples, compartidos con los tests de Rust.
    fs: { allow: ['..'] },
  },
});
