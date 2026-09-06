cpu     386
bits    32

extern      kernel_entry

global      _multiboot_entry
global      __page_directory
global      __page_table_identity
global      __page_table_first

; Multiboot kernels write this magic in their multiboot headers.
KERNEL_MAGIC    equ     0x1badb002
FLAGS           equ     (1<<0 | 1<<1)
CHECKSUM        equ     -(KERNEL_MAGIC + FLAGS)

section     .multiboot

    dd      KERNEL_MAGIC
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

alignb 4096
__page_directory:
resb    4 * 1024

; To identity map the first 4MB of the physical address space.
alignb 4096
__page_table_identity:
resb    4 * 1024

; To map the first 4MB of the physical address at a higher address, like 0xC0000000
alignb 4096
__page_table_first:
resb    4 * 1024

alignb 16
resb    2 * 1024 * 1024
__kernel_stack:
