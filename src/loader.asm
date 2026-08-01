cpu     386
bits    32

extern      kernel_main
global      loader
global      _idtr

MAGIC       equ     0x1badb002
FLAGS       equ     (1<<0 | 1<<1)
CHECKSUM    equ     -(MAGIC + FLAGS)

section     .multiboot

    dd      MAGIC
    dd      FLAGS
    dd      CHECKSUM

section     .text

loader:
    mov     esp, kernel_stack
    push    eax
    push    ebx
    call    kernel_main

_forever_loop:
    cli
    hlt
    jmp     _forever_loop

section    .bss

alignb 16
_idtr:          resb    6

alignb 16
resb    2 * 1024 * 1024
kernel_stack:
