# ADR 0004 — Cachés que miden, sin cambiar la temporización

- Estado: aceptada
- Fecha: 2026-11-28

## Contexto

La Fase 7 añade una caché de instrucciones y otra de datos configurables (número de conjuntos,
asociatividad, tamaño de línea, reemplazo LRU, FIFO o aleatorio, write-back o write-through) con
una vista de los conjuntos y un gráfico de aciertos. Hay que decidir si la caché solo mide o si
además cambia los ciclos de los modelos segmentados (paradas por fallo), dónde vive en el núcleo
y qué secuencia de accesos ve.

## Decisión

1. **La caché solo mide.** No cambia ni el estado arquitectónico ni los ciclos de ningún modelo:
   cuenta aciertos, fallos, expulsiones y escrituras a memoria. Así los cinco modelos siguen
   dando la misma traza que hoy, los riscv-tests no se tocan y el CPI de la tabla del pipeline
   sigue enseñando solo los riesgos. Lo que se aprende de una caché en clase (localidad,
   conflictos, efecto de la asociatividad y del tamaño de línea) se ve igual con las tasas de
   acierto. Si algún día se quiere latencia por fallo, será otro ADR.
2. **Ve la secuencia arquitectónica.** Se alimenta de lo retirado (`Retired`), en orden: por cada
   instrucción, primero la lectura de la instrucción en su `pc` en la caché de instrucciones y
   después, si la hay, su carga o su almacenamiento en la de datos. Las búsquedas que un salto
   descarta no cuentan. Consecuencia: para un mismo programa y una misma configuración, todos
   los modelos equivalentes dan las mismas estadísticas, y el paso atrás las recupera exactas.
3. **Dos cachés independientes**, cada una activable o no. Todos los parámetros son potencias de
   dos: de 1 a 1024 conjuntos, de 1 a 16 vías y líneas de 4 a 256 bytes. Un acceso de RISC-V va
   alineado a su tamaño (si no, es una excepción), así que nunca cruza dos líneas.
4. **Escritura**: write-back marca la línea sucia y escribe en memoria al expulsarla;
   write-through escribe en memoria en cada almacenamiento. La asignación en fallo de escritura
   (write-allocate) es un parámetro aparte; por defecto va con write-back y sin ella con
   write-through, que es la pareja de los libros.
5. **Reemplazo determinista**: LRU y FIFO por marcas de tiempo de acceso y de llenado; el
   aleatorio usa un xorshift de 32 bits con semilla fija que vive en el estado de la caché y se
   reinicia con ella. Siempre se llena antes una vía vacía.
6. **La E/S no se cachea**: los accesos a `[IO_BASE, 2^32)` van directos a memoria y se cuentan
   aparte. Los periféricos de la Fase 7 tienen que verse al momento.
7. **Historial acotado** para el gráfico: puntos acumulados (accesos, aciertos) cada N accesos,
   como mucho 256; al llenarse se queda uno de cada dos y N se duplica. Cabe en cada instantánea
   del paso atrás aunque el programa haga millones de accesos.
8. **Dónde vive**: `rv-core/src/cache.rs`, fuera del trait `Processor`. `CacheSystem::observe`
   recibe lo que devuelve `step`; quien ejecuta (la sesión de `rv-wasm`, la CLI) lo llama. Es
   `Clone` para guardarlo en las instantáneas junto a la máquina.

## Consecuencias

- La caché no puede romper la equivalencia entre modelos ni los riscv-tests.
- No se puede enseñar con ella que un fallo de caché frena el pipeline; la vista lo dice
  explícitamente para que nadie lo dé por hecho.
- La configuración se valida en el núcleo: un parámetro imposible es un error mostrado, no un
  pánico.
