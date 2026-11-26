# riscv-tests rv32ui/addi.S (commit 1b3f8bd665d79e5985753a558dae4e3718e2edc8), preprocesado por
# scripts/vendor-riscv-tests.sh. Licencia BSD: ver LICENSE en esta carpeta.
.text
.text; .globl _start; _start:
  test_2: li gp, 2; li x13, ((0x00000000) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x000) | (-(((0x000) >> 11) & 1) << 11));; li x7, ((0x00000000) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_3: li gp, 3; li x13, ((0x00000001) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x001) | (-(((0x001) >> 11) & 1) << 11));; li x7, ((0x00000002) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_4: li gp, 4; li x13, ((0x00000003) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x007) | (-(((0x007) >> 11) & 1) << 11));; li x7, ((0x0000000a) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_5: li gp, 5; li x13, ((0x0000000000000000) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x800) | (-(((0x800) >> 11) & 1) << 11));; li x7, ((0xfffffffffffff800) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_6: li gp, 6; li x13, ((0xffffffff80000000) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x000) | (-(((0x000) >> 11) & 1) << 11));; li x7, ((0xffffffff80000000) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_7: li gp, 7; li x13, ((0xffffffff80000000) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x800) | (-(((0x800) >> 11) & 1) << 11));; li x7, ((0xffffffff7ffff800) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_8: li gp, 8; li x13, ((0x00000000) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x7ff) | (-(((0x7ff) >> 11) & 1) << 11));; li x7, ((0x00000000000007ff) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_9: li gp, 9; li x13, ((0x7fffffff) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x000) | (-(((0x000) >> 11) & 1) << 11));; li x7, ((0x000000007fffffff) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_10: li gp, 10; li x13, ((0x7fffffff) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x7ff) | (-(((0x7ff) >> 11) & 1) << 11));; li x7, ((0x00000000800007fe) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_11: li gp, 11; li x13, ((0xffffffff80000000) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x7ff) | (-(((0x7ff) >> 11) & 1) << 11));; li x7, ((0xffffffff800007ff) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_12: li gp, 12; li x13, ((0x000000007fffffff) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x800) | (-(((0x800) >> 11) & 1) << 11));; li x7, ((0x000000007ffff7ff) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_13: li gp, 13; li x13, ((0x0000000000000000) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0xfff) | (-(((0xfff) >> 11) & 1) << 11));; li x7, ((0xffffffffffffffff) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_14: li gp, 14; li x13, ((0xffffffffffffffff) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x001) | (-(((0x001) >> 11) & 1) << 11));; li x7, ((0x0000000000000000) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_15: li gp, 15; li x13, ((0xffffffffffffffff) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0xfff) | (-(((0xfff) >> 11) & 1) << 11));; li x7, ((0xfffffffffffffffe) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_16: li gp, 16; li x13, ((0x7fffffff) & ((1 << (32 - 1) << 1) - 1)); addi x14, x13, ((0x001) | (-(((0x001) >> 11) & 1) << 11));; li x7, ((0x0000000080000000) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_17: li gp, 17; li x11, ((13) & ((1 << (32 - 1) << 1) - 1)); addi x11, x11, ((11) | (-(((11) >> 11) & 1) << 11));; li x7, ((24) & ((1 << (32 - 1) << 1) - 1)); bne x11, x7, fail;;
  test_18: li gp, 18; li x4, 0; 1: li x1, ((13) & ((1 << (32 - 1) << 1) - 1)); addi x14, x1, ((11) | (-(((11) >> 11) & 1) << 11)); addi x6, x14, 0; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b; li x7, ((24) & ((1 << (32 - 1) << 1) - 1)); bne x6, x7, fail;;
  test_19: li gp, 19; li x4, 0; 1: li x1, ((13) & ((1 << (32 - 1) << 1) - 1)); addi x14, x1, ((10) | (-(((10) >> 11) & 1) << 11)); nop; addi x6, x14, 0; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b; li x7, ((23) & ((1 << (32 - 1) << 1) - 1)); bne x6, x7, fail;;
  test_20: li gp, 20; li x4, 0; 1: li x1, ((13) & ((1 << (32 - 1) << 1) - 1)); addi x14, x1, ((9) | (-(((9) >> 11) & 1) << 11)); nop; nop; addi x6, x14, 0; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b; li x7, ((22) & ((1 << (32 - 1) << 1) - 1)); bne x6, x7, fail;;
  test_21: li gp, 21; li x4, 0; 1: li x1, ((13) & ((1 << (32 - 1) << 1) - 1)); addi x14, x1, ((11) | (-(((11) >> 11) & 1) << 11)); addi x4, x4, 1; li x5, 2; bne x4, x5, 1b; li x7, ((24) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_22: li gp, 22; li x4, 0; 1: li x1, ((13) & ((1 << (32 - 1) << 1) - 1)); nop; addi x14, x1, ((10) | (-(((10) >> 11) & 1) << 11)); addi x4, x4, 1; li x5, 2; bne x4, x5, 1b; li x7, ((23) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_23: li gp, 23; li x4, 0; 1: li x1, ((13) & ((1 << (32 - 1) << 1) - 1)); nop; nop; addi x14, x1, ((9) | (-(((9) >> 11) & 1) << 11)); addi x4, x4, 1; li x5, 2; bne x4, x5, 1b; li x7, ((22) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_24: li gp, 24; addi x1, x0, ((32) | (-(((32) >> 11) & 1) << 11));; li x7, ((32) & ((1 << (32 - 1) << 1) - 1)); bne x1, x7, fail;;
  test_25: li gp, 25; li x1, ((33) & ((1 << (32 - 1) << 1) - 1)); addi x0, x1, ((50) | (-(((50) >> 11) & 1) << 11));; li x7, ((0) & ((1 << (32 - 1) << 1) - 1)); bne x0, x7, fail;;
  bne x0, gp, pass; fail: mv a0, gp; li a7, 93; ecall; pass: li a0, 0; li a7, 93; ecall
  .data
.data
