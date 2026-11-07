; Standalone local bench exercise. Writes the sum 1..10 to SRAM at 0x8700.
; asm archive/bench-example.asm 0x8800 example.bin
; lab load example.bin 0x8800
; lab pc 0x8800
; lab step 65
; lab mem 0x8700 4
movi r0, 0
movi r1, 1
movi r2, 11
loop:
add r0, r0, r1
addi r1, r1, 1
cmp r1, r2
jlo loop
movi r3, 0x8700
st r0, r3, 0
halt
