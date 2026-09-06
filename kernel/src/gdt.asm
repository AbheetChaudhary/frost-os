cpu     386
bits    32

global _gdtr
global _gdt_flush

_gdt_flush:
    lgdt    [_gdtr]

    jmp     0x08:farjump

farjump:
    mov     ax, 0x10
    mov     ds, ax
    mov     es, ax
    mov     fs, ax
    mov     gs, ax
    mov     ss, ax

    ret

align 16
_gdtr:      db  6
