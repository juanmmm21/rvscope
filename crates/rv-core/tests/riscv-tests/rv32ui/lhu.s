# riscv-tests rv32ui/lhu.S (commit 1b3f8bd665d79e5985753a558dae4e3718e2edc8), preprocesado por
# scripts/vendor-riscv-tests.sh. Licencia BSD: ver LICENSE en esta carpeta.
.text
.text; .globl _start; _start:
  test_2: li gp, 2; li x15, 0x00000000000000ff; la x2, tdat; lhu x14, 0(x2);; li x7, ((0x00000000000000ff) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_3: li gp, 3; li x15, 0x000000000000ff00; la x2, tdat; lhu x14, 2(x2);; li x7, ((0x000000000000ff00) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_4: li gp, 4; li x15, 0x0000000000000ff0; la x2, tdat; lhu x14, 4(x2);; li x7, ((0x0000000000000ff0) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_5: li gp, 5; li x15, 0x000000000000f00f; la x2, tdat; lhu x14, 6(x2);; li x7, ((0x000000000000f00f) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_6: li gp, 6; li x15, 0x00000000000000ff; la x2, tdat4; lhu x14, -6(x2);; li x7, ((0x00000000000000ff) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_7: li gp, 7; li x15, 0x000000000000ff00; la x2, tdat4; lhu x14, -4(x2);; li x7, ((0x000000000000ff00) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_8: li gp, 8; li x15, 0x0000000000000ff0; la x2, tdat4; lhu x14, -2(x2);; li x7, ((0x0000000000000ff0) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_9: li gp, 9; li x15, 0x000000000000f00f; la x2, tdat4; lhu x14, 0(x2);; li x7, ((0x000000000000f00f) & ((1 << (32 - 1) << 1) - 1)); bne x14, x7, fail;;
  test_10: li gp, 10; la x1, tdat; addi x1, x1, -32; lhu x5, 32(x1);; li x7, ((0x00000000000000ff) & ((1 << (32 - 1) << 1) - 1)); bne x5, x7, fail;
  test_11: li gp, 11; la x1, tdat; addi x1, x1, -5; lhu x5, 7(x1);; li x7, ((0x000000000000ff00) & ((1 << (32 - 1) << 1) - 1)); bne x5, x7, fail;
  test_12: li gp, 12; li x4, 0; 1: la x13, tdat2; lhu x14, 2(x13); addi x6, x14, 0; li x7, 0x0000000000000ff0; bne x6, x7, fail; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;;
  test_13: li gp, 13; li x4, 0; 1: la x13, tdat3; lhu x14, 2(x13); nop; addi x6, x14, 0; li x7, 0x000000000000f00f; bne x6, x7, fail; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;;
  test_14: li gp, 14; li x4, 0; 1: la x13, tdat1; lhu x14, 2(x13); nop; nop; addi x6, x14, 0; li x7, 0x000000000000ff00; bne x6, x7, fail; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;;
  test_15: li gp, 15; li x4, 0; 1: la x13, tdat2; lhu x14, 2(x13); li x7, 0x0000000000000ff0; bne x14, x7, fail; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;
  test_16: li gp, 16; li x4, 0; 1: la x13, tdat3; nop; lhu x14, 2(x13); li x7, 0x000000000000f00f; bne x14, x7, fail; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;
  test_17: li gp, 17; li x4, 0; 1: la x13, tdat1; nop; nop; lhu x14, 2(x13); li x7, 0x000000000000ff00; bne x14, x7, fail; addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;
  test_18: li gp, 18; la x5, tdat; lhu x2, 0(x5); li x2, 2;; li x7, ((2) & ((1 << (32 - 1) << 1) - 1)); bne x2, x7, fail;
  test_19: li gp, 19; la x5, tdat; lhu x2, 0(x5); nop; li x2, 2;; li x7, ((2) & ((1 << (32 - 1) << 1) - 1)); bne x2, x7, fail;
  bne x0, gp, pass; fail: mv a0, gp; li a7, 93; ecall; pass: li a0, 0; li a7, 93; ecall
  .data
.data
tdat:
tdat1: .half 0x00ff
tdat2: .half 0xff00
tdat3: .half 0x0ff0
tdat4: .half 0xf00f
