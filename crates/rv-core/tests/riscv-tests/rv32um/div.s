# riscv-tests rv32um/div.S (commit 1b3f8bd665d79e5985753a558dae4e3718e2edc8), preprocesado por
# scripts/vendor-riscv-tests.sh. Licencia BSD: ver LICENSE en esta carpeta.
.text
.text; .globl _start; _start:
  test_2: li gp, 2; li x11, ((20) & ((1 << (32 - 1) << 1) - 1)); li x12, ((6) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((3) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_3: li gp, 3; li x11, ((-20) & ((1 << (32 - 1) << 1) - 1)); li x12, ((6) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((-3) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_4: li gp, 4; li x11, ((20) & ((1 << (32 - 1) << 1) - 1)); li x12, ((-6) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((-3) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_5: li gp, 5; li x11, ((-20) & ((1 << (32 - 1) << 1) - 1)); li x12, ((-6) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((3) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_6: li gp, 6; li x11, ((-1<<31) & ((1 << (32 - 1) << 1) - 1)); li x12, ((1) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((-1<<31) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_7: li gp, 7; li x11, ((-1<<31) & ((1 << (32 - 1) << 1) - 1)); li x12, ((-1) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((-1<<31) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_8: li gp, 8; li x11, ((-1<<31) & ((1 << (32 - 1) << 1) - 1)); li x12, ((0) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((-1) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_9: li gp, 9; li x11, ((1) & ((1 << (32 - 1) << 1) - 1)); li x12, ((0) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((-1) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_10: li gp, 10; li x11, ((0) & ((1 << (32 - 1) << 1) - 1)); li x12, ((0) & ((1 << (32 - 1) << 1) - 1)); div x14, x11, x12;; li x7, ((-1) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  bne x0, gp, pass; fail: mv a0, gp; li a7, 93; ecall; pass: li a0, 0; li a7, 93; ecall
  .data
.data
