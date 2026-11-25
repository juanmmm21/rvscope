# Factorial recursivo de N; el resultado queda en a0.
        .data
n:      .word 6

        .text
main:
        lw    a0, n
        call  factorial
        li    a7, 10            # salir
        ecall

# factorial(a0) -> a0
factorial:
        addi  sp, sp, -8
        sw    ra, 4(sp)
        sw    a0, 0(sp)
        li    t0, 1
        ble   a0, t0, base
        addi  a0, a0, -1
        call  factorial
        lw    t1, 0(sp)
        mul   a0, a0, t1
        j     done
base:
        li    a0, 1
done:
        lw    ra, 4(sp)
        addi  sp, sp, 8
        ret
