# riscv-tests rv32ui/auipc.S (commit 1b3f8bd665d79e5985753a558dae4e3718e2edc8), preprocesado por
# scripts/vendor-riscv-tests.sh. Licencia BSD: ver LICENSE en esta carpeta.
.text
.text; .globl _start; _start:
  test_2: li gp, 2; .align 3; lla a0, 1f + 10000; jal a1, 1f; 1: sub a0, a0, a1;; li x7, ((10000) & ((1 << (32 - 1) << 1) - 1)); bne a0, x7, fail;
  test_3: li gp, 3; .align 3; lla a0, 1f - 10000; jal a1, 1f; 1: sub a0, a0, a1;; li x7, ((-10000) & ((1 << (32 - 1) << 1) - 1)); bne a0, x7, fail;
  bne x0, gp, pass; fail: mv a0, gp; li a7, 93; ecall; pass: li a0, 0; li a7, 93; ecall
  .data
.data
