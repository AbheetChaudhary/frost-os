ENTRY(_multiboot_entry)
OUTPUT_FORMAT(elf32-i386)
OUTPUT_ARCH(i386:i386)

KERNEL_VMA = 0xC0000000;
KERNEL_LMA = 0x100000;
KERNEL_OFFSET = KERNEL_VMA - KERNEL_LMA;

SECTIONS
{
    /* . = 0x100000; */
    . = KERNEL_VMA;

    .text : AT(ADDR(.text) - KERNEL_OFFSET)
    {
        *(.multiboot)
        *(.text*)
        *(.rodata*)
    }

    .data : AT(ADDR(.data) - KERNEL_OFFSET)
    {
        start_ctors = .;
        KEEP(*(.init_array));
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*)));
        end_ctors   = .;

        *(.data*)
    }

    .bss : AT(ADDR(.bss) - KERNEL_OFFSET)
    {
        *(.bss*)
    }

    /DISCARD/ :
    {
        *(.fini_array*)
        *(.comment)
    }
}
