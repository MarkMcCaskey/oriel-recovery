; Maintenance module source export, 1.2 (prelaunch). Not the 1.3 object.
; External symbols supplied by the application linker.
.equ address 0x00000420
.equ service_table 0x1800
.org 0xb00
service_dispatch:
call address
cmpi r0, 4
jhs done
shl r0, r0, 2
movi r1, service_table
add r1, r1, r0
ld r0, r1, 0
callr r0
done:
ret
