# riscv-tests rv32ui/jalr.S (commit 1b3f8bd665d79e5985753a558dae4e3718e2edc8), preprocesado por
# scripts/vendor-riscv-tests.sh. Licencia BSD: ver LICENSE en esta carpeta.
.text
.text; .globl _start; _start:
test_2:
  li gp, 2
  li t0, 0
  la t1, target_2
  jalr t0, t1, 0
linkaddr_2:
  j fail
target_2:
  la t1, linkaddr_2
  bne t0, t1, fail
test_3:
  li gp, 3
  la t0, target_3
  jalr t0, t0, 0
linkaddr_3:
  j fail
target_3:
  la t1, linkaddr_3
  bne t0, t1, fail
  test_4: li gp, 4; li x4, 0; 1: la x6, 2f; jalr x13, x6, 0; bne x0, gp, fail; 2: addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;
  test_5: li gp, 5; li x4, 0; 1: la x6, 2f; nop; jalr x13, x6, 0; bne x0, gp, fail; 2: addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;
  test_6: li gp, 6; li x4, 0; 1: la x6, 2f; nop; nop; jalr x13, x6, 0; bne x0, gp, fail; 2: addi x4, x4, 1; li x5, 2; bne x4, x5, 1b;
  .option push
  .align 2
  .option norvc
  test_7: li gp, 7; li t0, 1; la t1, 1f; jr t1, -4; addi t0, t0, 1; addi t0, t0, 1; addi t0, t0, 1; addi t0, t0, 1; 1: addi t0, t0, 1; addi t0, t0, 1;; li x7, ((4) & ((1 << (32 - 1) << 1) - 1)); bne t0, x7, fail;
  .option pop
  bne x0, gp, pass; fail: mv a0, gp; li a7, 93; ecall; pass: li a0, 0; li a7, 93; ecall
  .data
.data
