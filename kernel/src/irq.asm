cpu     386
bits    32


global _irq0
global _irq1
global _irq2
global _irq3
global _irq4
global _irq5
global _irq6
global _irq7
global _irq8
global _irq9
global _irq10
global _irq11
global _irq12
global _irq13
global _irq14
global _irq15


extern irq_handler


; Takes one argument, the index in the IDT for this IRQ.
;
; Disables interrupt, pushes dummy error code, and jumps to the common stub.
%macro  irq_setup   1

    cli
    push    0
    push    %1
    jmp     irq_common_stub

%endmacro


; Since IDT entries 0-31 are reserved for processor exceptions. So hardware IRQs
; are being mapped to IDT entries 32-47.
_irq0:      irq_setup   32
_irq1:      irq_setup   33
_irq2:      irq_setup   34
_irq3:      irq_setup   35
_irq4:      irq_setup   36
_irq5:      irq_setup   37
_irq6:      irq_setup   38
_irq7:      irq_setup   39
_irq8:      irq_setup   40
_irq9:      irq_setup   41
_irq10:     irq_setup   42
_irq11:     irq_setup   43
_irq12:     irq_setup   44
_irq13:     irq_setup   45
_irq14:     irq_setup   46
_irq15:     irq_setup   47


irq_common_stub:
    pusha
    push    ds
    push    es
    push    fs
    push    gs

    mov     ax, 0x10
    mov     ds, ax
    mov     es, ax
    mov     fs, ax
    mov     gs, ax
    mov     eax, esp
    push    eax

    ; C code following the sysV ABI requires DF to be clear on function entry
    cld

    ; Indirect near call to `irq_handler`.
    ;
    ; For a flat 32-bit kernel, the called function could be anywhere in the
    ; 4GiB range, `call` instruction will only push the EIP and it will still
    ; work correctly with the `extern "C"` abi.
    mov     eax, irq_handler
    call    eax

    pop     eax
    pop     gs
    pop     fs
    pop     es
    pop     ds
    popa

    ; Reclaim 8 bytes pushed by irq_setup before calling the common stub.
    add     esp, 8

    iretd
