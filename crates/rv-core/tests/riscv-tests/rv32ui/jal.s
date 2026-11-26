# riscv-tests rv32ui/jal.S (commit 1b3f8bd665d79e5985753a558dae4e3718e2edc8), preprocesado por
# scripts/vendor-riscv-tests.sh. Licencia BSD: ver LICENSE en esta carpeta.
.text
.text; .globl _start; _start:
test_2:
  li gp, 2
  li ra, 0
  jal x4, target_2
linkaddr_2:
  nop
  nop
  j fail
target_2:
  la x2, linkaddr_2
  bne x2, x4, fail
  test_3: li gp, 3; li ra, 1; jal x0, 1f; addi ra, ra, 1; addi ra, ra, 1; addi ra, ra, 1; addi ra, ra, 1; 1: addi ra, ra, 1; addi ra, ra, 1;; li x7, ((3) & ((1 << (32 - 1) << 1) - 1)); bne ra, x7, fail;
  bne x0, gp, pass; fail: mv a0, gp; li a7, 93; ecall; pass: li a0, 0; li a7, 93; ecall
  .data
.data
