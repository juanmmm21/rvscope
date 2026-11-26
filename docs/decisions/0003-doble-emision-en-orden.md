# ADR 0003 — Doble emisión en orden

- Estado: aceptada
- Fecha: 2026-11-26

## Contexto

El último modelo de la Fase 3 es un procesador superescalar sencillo: dos instrucciones por ciclo,
en orden, sobre el pipeline de cinco etapas con forwarding. Hay que fijar qué pares pueden ir
juntos, cómo se buscan, qué pasa con los saltos y las excepciones y cómo se retiran dos
instrucciones en un ciclo sin perder las paradas precisas del ADR 0002, que sigue valiendo para
todo lo que este ADR no cambia.

## Decisión

1. **Dos vías de cinco etapas** (IF, ID, EX, MEM, WB) con dos ALU completas (también para `mul` y
   `div`), un solo puerto de memoria de datos y una sola unidad de saltos. Cada registro de
   segmentación lleva dos huecos: el de la vía 0 es siempre la instrucción más antigua.
2. **Búsqueda en orden libre**: IF/ID es una ventana de dos instrucciones consecutivas. Si en un
   ciclo se emiten las dos, se buscan dos nuevas; si se emite una, la que queda pasa a ser la más
   antigua y se busca una sola detrás; si no se emite ninguna, no se busca. Los pares no tienen
   que empezar en una dirección múltiplo de 8: así se emparejan más instrucciones y el modelo no
   depende de dónde caiga el código.
3. **Reglas de emisión** (en ID, por orden): la más antigua se emite si no tiene un riesgo de
   carga-uso con una carga que esté en EX (en cualquiera de las dos vías). La segunda la acompaña
   solo si además:
   - no lee el registro que escribe la primera (no hay forwarding dentro del par);
   - no accede a memoria si la primera también lo hace (un solo puerto);
   - la primera no es un salto, `jal` ni `jalr`: un salto cierra el par, así que lo que va detrás
     de él nunca está en EX a su lado y el vaciado es el mismo que en el ADR 0002 (IF e ID);
   - ninguna de las dos es una `ecall` ni una instrucción que ya ha fallado: esas van solas.
   Dos instrucciones que escriben el mismo registro sí pueden ir juntas: WB escribe la más
   antigua primero y el forwarding prefiere la más joven.
4. **Forwarding** a EX desde los cuatro huecos de EX/MEM y MEM/WB, del más reciente al más
   antiguo: EX/MEM vía 1, EX/MEM vía 0, MEM/WB vía 1, MEM/WB vía 0. Carga-uso sigue costando una
   burbuja.
5. **Paradas precisas**: MEM y WB recorren la vía 0 antes que la 1. Si la vía 0 falla en WB, la 1
   no escribe nada; si la 0 se retira y la 1 falla, en ese ciclo se retira solo la primera y se
   para (el ciclo cuenta porque algo se retiró). Una instrucción que falla en IF o ID viaja sola
   hasta WB y nada se busca mientras está en vuelo.
6. **`ecall` serializa** igual que en el ADR 0002: va sola, nada se busca ni se emite detrás de
   ella hasta que se retira, y se atiende en WB con todo lo anterior ya escrito.
7. **Equivalencia**: entra en el arnés de riscv-tests y debe acabar en el mismo estado
   arquitectónico que el de un ciclo, con el mismo número de retiradas.

## Consecuencias

- La semántica sigue siendo la de `exec.rs`; las piezas comunes del pipeline (registros de
  segmentación, decodificación, MEM y WB) se comparten con el modelo de cinco etapas.
- El modelo enseña los límites de la doble emisión: dependencias dentro del par, el puerto único
  de memoria y los saltos dejan huecos en la vía 1, y el CPI baja de 1 solo con código
  independiente.
