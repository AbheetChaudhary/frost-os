cpu     386
bits    32


extern exception_handler


; ISR's 0-31 are reserved for processor exceptions.
global  _isr0
global  _isr1
global  _isr2
global  _isr3
global  _isr4
global  _isr5
global  _isr6
global  _isr7
global  _isr8
global  _isr9
global  _isr10
global  _isr11
global  _isr12
global  _isr13
global  _isr14
global  _isr15
global  _isr16
global  _isr17
global  _isr18
global  _isr19
global  _isr20
global  _isr21
global  _isr22
global  _isr23
global  _isr24
global  _isr25
global  _isr26
global  _isr27
global  _isr28
global  _isr29
global  _isr30
global  _isr31


; Used for an exception that does not automatically pushes an error on the stack. So
; this macro pushes one itself before pushing the interrupt number.
;
; Takes as argument the index in the IDT for this exception's ISR, aka the
; interrupt number.
%macro  isr_setup_no_error  1

    cli
    push    0
    push    %1
    jmp     isr_common_stub

%endmacro

; Used for an exception that automatically pushes an error on the stack. So this 
; macro just pushes the interrupt number on the stack.
;
; Takes as argument the index in the IDT for this exception's ISR, aka the 
; interrupt number.
%macro  isr_setup_has_error     1

    cli
    push    %1
    jmp     isr_common_stub

%endmacro


_isr0:       isr_setup_no_error      0
_isr1:       isr_setup_no_error      1
_isr2:       isr_setup_no_error      2
_isr3:       isr_setup_no_error      3
_isr4:       isr_setup_no_error      4
_isr5:       isr_setup_no_error      5
_isr6:       isr_setup_no_error      6
_isr7:       isr_setup_no_error      7

_isr8:       isr_setup_has_error      8

_isr9:       isr_setup_no_error      9

_isr10:      isr_setup_has_error      10
_isr11:      isr_setup_has_error      11
_isr12:      isr_setup_has_error      12
_isr13:      isr_setup_has_error      13
_isr14:      isr_setup_has_error      14

_isr15:      isr_setup_no_error      15
_isr16:      isr_setup_no_error      16
_isr17:      isr_setup_no_error      17
_isr18:      isr_setup_no_error      18
_isr19:      isr_setup_no_error      19
_isr20:      isr_setup_no_error      20
_isr21:      isr_setup_no_error      21
_isr22:      isr_setup_no_error      22
_isr23:      isr_setup_no_error      23
_isr24:      isr_setup_no_error      24
_isr25:      isr_setup_no_error      25
_isr26:      isr_setup_no_error      26
_isr27:      isr_setup_no_error      27
_isr28:      isr_setup_no_error      28
_isr29:      isr_setup_no_error      29
_isr30:      isr_setup_no_error      30
_isr31:      isr_setup_no_error      31


isr_common_stub:
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

    mov     eax, exception_handler
    call    eax

    pop     eax
    pop     gs
    pop     fs
    pop     es
    pop     ds
    popa

    ; Clean up the push error code and ISR number.
    add     esp, 8

    ; Pop CS, EIP, EFLAGS, SS, and ESP, and re-enable interrupts.
    iretd
