# ADR 0002 — Pipeline de cinco etapas: temporización, paradas y equivalencia

- Estado: aceptada
- Fecha: 2026-11-26

## Contexto

La Fase 3 añade procesadores segmentados de cinco etapas (IF, ID, EX, MEM, WB). El primero no
tiene ni forwarding ni detección de riesgos: existe para que se vea qué pasa cuando una
instrucción lee un registro que la anterior aún no ha escrito. Eso choca con la regla del ADR
0001 de que todos los modelos acaban en el mismo estado que el de un ciclo, y hay que fijar
cuándo se resuelven los saltos, dónde se atienden las `ecall` y cómo se para con precisión.

## Decisión

1. **Banco de registros con escritura antes que lectura**: WB escribe en la primera mitad del
   ciclo e ID lee en la segunda. Una instrucción ve el resultado de la que va tres por delante;
   las dos inmediatamente anteriores le dan el valor viejo si no hay detección ni forwarding.
2. **Memorias de instrucciones y de datos separadas**: no hay riesgo estructural entre IF y MEM.
3. **Saltos resueltos en EX con predicción de no tomado**: un salto tomado (y `jal`/`jalr`, que
   siempre lo son) vacía las dos instrucciones que van detrás y busca en el destino. Esto lo
   hacen todos los modelos de cinco etapas, también el que no detecta riesgos: los riesgos que se
   dejan sin tratar son los de datos; sin vaciado, cualquier bucle ejecutaría instrucciones de
   más y el modelo no serviría para nada.
4. **Paradas precisas en WB**: la excepción se detecta en su etapa (búsqueda en IF; instrucción
   ilegal y `ebreak` en ID; destino de salto no alineado en EX; acceso a datos en MEM) y viaja
   con la instrucción hasta WB, donde el procesador se para. Las etapas se evalúan de WB hacia
   IF dentro del ciclo, así que nada más joven llega a escribir memoria ni registros. Mientras hay
   una excepción en vuelo no se busca nada nuevo.
5. **`ecall` serializa**: al decodificar una `ecall` se deja de buscar hasta que se retira; se
   atiende en WB con los registros ya escritos por todo lo anterior y la búsqueda sigue en la
   instrucción siguiente. Cuesta unos ciclos, pero la consola no puede ver estados a medias y la
   espera de entrada congela el pipeline entero sin contar ciclos.
6. **El `pc` arquitectónico es el de retirada**: `ArchState.pc` es el `next_pc` de la última
   instrucción retirada; el `pc` de búsqueda es interno del modelo.
7. **Equivalencia**: la regla del mismo estado final que el de un ciclo se exige a los modelos
   que tratan los riesgos de datos (detección con burbujas, forwarding, doble emisión) y con
   ellos se pasan los riscv-tests. El modelo sin detección solo la cumple con programas sin
   riesgos de datos (dos instrucciones independientes o `nop` entre productor y consumidor, y lo
   mismo tras una carga); se prueba con programas así y con otros que muestran el valor viejo.

## Consecuencias

- El modelo sin detección no entra en el arnés de riscv-tests: sus resultados distintos son su
  razón de ser, no un fallo.
- Los modelos con detección y con forwarding parten del mismo esqueleto y solo cambian la
  unidad de riesgos, así que el vaciado, las paradas y las `ecall` se comportan igual en todos.
- Las `ecall` cuestan ciclos de más en los modelos segmentados; las estadísticas lo reflejarán.
