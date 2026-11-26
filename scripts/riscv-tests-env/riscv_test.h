// Entorno propio de rvscope para los riscv-tests: sustituye al «p» del repositorio oficial,
// que usa CSR, interrupciones y la «tohost» de un simulador concreto. Aquí el resultado sale
// por la llamada al sistema de salida: código 0 si pasa y el número del caso que falla si no.

#ifndef RVSCOPE_RISCV_TEST_H
#define RVSCOPE_RISCV_TEST_H

#define RVTEST_RV32U .text
#define RVTEST_RV64U RVTEST_RV32U
#define RVTEST_RV32M .text
#define RVTEST_RV64M RVTEST_RV32M

#define TESTNUM gp

#define RVTEST_CODE_BEGIN \
  .text; \
  .globl _start; \
_start:

#define RVTEST_CODE_END

#define RVTEST_PASS \
  li a0, 0; \
  li a7, 93; \
  ecall

#define RVTEST_FAIL \
  mv a0, TESTNUM; \
  li a7, 93; \
  ecall

#define RVTEST_DATA_BEGIN .data
#define RVTEST_DATA_END

#endif
