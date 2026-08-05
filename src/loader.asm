cpu     386
bits    32

extern      kernel_entry

global      _multiboot_entry

MAGIC       equ     0x1badb002
FLAGS       equ     (1<<0 | 1<<1)
CHECKSUM    equ     -(MAGIC + FLAGS)

section     .multiboot

    dd      MAGIC
    dd      FLAGS
    dd      CHECKSUM

section     .text

; This is where the grub bootloader drops us. We setup the stack and call
; `kernel_entry` 
_multiboot_entry:
    mov     esp, __kernel_stack
    push    eax
    push    ebx
    call    kernel_entry

_forever_loop:
    cli
    hlt
    jmp     _forever_loop


section    .bss

alignb 16
resb    2 * 1024 * 1024
__kernel_stack:
