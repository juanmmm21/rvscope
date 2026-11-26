/// <reference types="vitest/config" />
import { defineConfig } from 'vite';

export default defineConfig({
  build: {
    target: 'es2022',
  },
  server: {
    // Los ejemplos viven en ../examples, compartidos con los tests de Rust.
    fs: { allow: ['..'] },
  },
  test: {
    // Los de e2e/ son de Playwright y corren contra la web compilada.
    include: ['src/**/*.test.ts'],
  },
});
