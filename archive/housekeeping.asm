; Recorder task, FSW 1.3
.equ emit 0x900
.org 0x1000

housekeeping:
movi r4, 0xa800
movi r0, 0x314b484f
st r0, r4, 0
ld r0, r6, 0x20
st r0, r4, 4
ld r0, r6, 0x50
st r0, r4, 8
ld r0, r6, 0x54
st r0, r4, 12
ld r0, r6, 0x24
st r0, r4, 16
movi r3, 0x8004
ld r0, r3, 0
addi r0, r0, 1
st r0, r3, 0
st r0, r4, 20
movi r0, 1
st r0, r4, 24
movi r0, 0x0103
st r0, r4, 28
ld r0, r6, 0x58
st r0, r4, 32
movi r0, 0
movi r1, 0
sum_loop:
add r2, r4, r1
ld r2, r2, 0
add r0, r0, r2
addi r1, r1, 4
cmpi r1, 36
jlo sum_loop
st r0, r4, 36
mov r0, r4
movi r1, 40
movi r2, 0x90
call emit
ret
