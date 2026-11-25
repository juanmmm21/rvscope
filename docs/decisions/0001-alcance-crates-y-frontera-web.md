# ADR 0001 — Alcance, crates y frontera entre el núcleo y la web

- Estado: aceptada
- Fecha: 2026-11-25

## Contexto

rvscope es un simulador de RISC-V para aprender y practicar ensamblador: hay que poder escribir un
programa, ensamblarlo y verlo ejecutarse instrucción a instrucción en varios modelos de
procesador, con la ruta de datos, los registros, la memoria y la caché a la vista. Tiene que
funcionar en el navegador sin instalar nada y dar exactamente el mismo resultado cada vez.

## Decisión

1. **Todo en el cliente.** El simulador es Rust compilado a WebAssembly; la interfaz es TypeScript
   con Vite. No hay servidor: la web se puede servir como ficheros estáticos.
2. **Un workspace de Cargo con cinco crates** de responsabilidad única:
   - `rv-isa`: la tabla de instrucciones es la fuente única de verdad. De ella salen el
     decodificador, el codificador, el ensamblador y la chuleta de la interfaz, así que la chuleta
     nunca puede describir una instrucción que el simulador no admite.
   - `rv-asm`: ensamblador con directivas, etiquetas y pseudoinstrucciones; errores con posición.
   - `rv-core`: memoria, modelos de procesador, caché, periféricos y llamadas al sistema.
   - `rv-wasm`: la única pieza que conoce `wasm-bindgen`.
   - `rv-cli`: ejecuta programas desde la terminal; es lo que usan los tests de conformidad.
3. **Alcance de la ISA**: RV32I y la extensión M primero. La extensión C y RV64 quedan fuera hasta
   que lo anterior esté terminado y probado.
4. **Modelos de procesador** detrás de un mismo trait: un ciclo (referencia), cinco etapas sin
   forwarding ni detección de riesgos, con detección de riesgos, con forwarding y detección, y
   doble emisión en orden. Todos deben producir el mismo estado arquitectónico final que el de un
   ciclo; lo que cambia es la temporización y lo que se ve.
5. **Frontera estrecha**: la web pide operaciones (`assemble`, `reset`, `step`, `step_back`,
   `run_until`) y recibe instantáneas serializadas (registros, memoria por rangos, estado de cada
   etapa con sus señales, estadísticas, caché). La web no reimplementa nada del procesador.
6. **Paso atrás** por instantáneas cada N ciclos y re-ejecución determinista desde la más cercana,
   en vez de guardar un registro de deshacer por instrucción.
7. **Conformidad** con los riscv-tests oficiales (rv32ui, rv32um) en todos los modelos, en CI.

## Consecuencias

- La lógica difícil se prueba en Rust sin navegador, y la CLI permite depurar una traza fuera de la
  interfaz.
- Añadir un modelo de procesador nuevo no toca la web más allá de su diagrama.
- El tamaño del wasm y el coste de serializar instantáneas hay que vigilarlos: la interfaz pide
  solo los rangos de memoria visibles.
