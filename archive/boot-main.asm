; Export of FSW 1.3 core. Maintenance module source not in archive.
.equ service_dispatch 0xb00
.equ housekeeping 0x1000
.org 0
jmp boot
jmp timer
.org 0x100
boot:
movi r6, 0xf000
ei
main:
movi r6, 0xf000
ld r5, r6, 0
cmpi r5, 0
jz idle
movi r4, 0x8200
movi r3, 0
receive:
ld r0, r6, 4
stb r0, r4, 0
addi r4, r4, 1
addi r3, r3, 1
cmp r3, r5
jlo receive
movi r0, 1
st r0, r6, 8
movi r4, 0x8200
ldb r0, r4, 0
cmpi r0, 1
jz ping
cmpi r0, 2
jz read_cmd
cmpi r0, 3
jz write_cmd
cmpi r0, 5
jz service_cmd
cmpi r0, 6
jz reset_cmd
cmpi r0, 7
jz update_cmd
cmpi r0, 8
jz update_cmd
cmpi r0, 9
jz update_cmd
jmp error
idle:
wait
jmp main
ping:
movi r0, 0x1900
movi r1, 8
movi r2, 0x81
call emit
jmp main
read_cmd:
cmpi r5, 5
jnz error
call address
ldb r1, r4, 3
ldb r2, r4, 4
shl r2, r2, 8
or r1, r1, r2
cmpi r1, 129
jhs error
add r2, r0, r1
cmpi r2, 0xc001
jhs error
movi r2, 0x82
call emit
jmp main
write_cmd:
cmpi r5, 4
jlo error
cmpi r5, 132
jhs error
call address
cmpi r0, 0x8800
jlo error
addi r1, r5, -3
add r2, r0, r1
cmpi r2, 0x9001
jhs error
addi r4, r4, 3
write_loop:
ldb r2, r4, 0
stb r2, r0, 0
addi r4, r4, 1
addi r0, r0, 1
addi r1, r1, -1
cmpi r1, 0
jnz write_loop
movi r0, 0x8200
movi r1, 3
movi r2, 0x83
call emit
jmp main
service_cmd:
cmpi r5, 3
jnz error
call service_dispatch
jmp main
reset_cmd:
cmpi r5, 1
jnz error
movi r0, 1
st r0, r6, 0x40
jmp main
update_cmd:
movi r0, 0x1910
movi r1, 2
movi r2, 0xe0
call emit
jmp main
error:
movi r0, 0x1912
movi r1, 2
movi r2, 0xe0
call emit
jmp main
address:
ldb r0, r4, 1
ldb r2, r4, 2
shl r2, r2, 8
or r0, r0, r2
ret
.org 0x900
emit:
st r0, r6, 0x10
st r1, r6, 0x14
st r2, r6, 0x1c
movi r0, 1
st r0, r6, 0x18
ret
.org 0xa00
timer:
movi r6, 0xf000
movi r0, 0x51ac
st r0, r6, 0x2c
movi r4, 0x8000
ld r0, r4, 0
addi r0, r0, 1
st r0, r4, 0
cmpi r0, 30
jlo timer_exit
movi r0, 0
st r0, r4, 0
call housekeeping
timer_exit:
iret
