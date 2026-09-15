use core::arch::asm;

use crate::{address, acpi, serial, page_tables, cpu, utils, isr, gdt};
use address::{PhysAddr32, PhysAddr64, PhysAddr};

pub fn mp_init(madt_ptr: *const acpi::RawMadt) {
    /*
    assert!(cpu::is_modern_ia_processor(), "Not a modern processor! Don't \
        know how to boot multiple cores");
    */

    // let madt_ref = unsafe { acpi::MadtRef::from_raw_ptr(madt_ptr) };

    let ia32_apic_base_msr = unsafe { cpu::rdmsr(0x1b) };

    let is_bsp = ((ia32_apic_base_msr >> 8) & 0x01) == 1;
    assert!(is_bsp, "mp_init() must be called from BSP only");

    let global_apic_enable = ((ia32_apic_base_msr >> 11) & 0x01) == 1;
    assert!(global_apic_enable, "Global APIC is not enabled");

    // Assuming reserved fields are all set to 0.
    let apic_base_phys: usize = ((ia32_apic_base_msr >> 12) << 12)
        .try_into().unwrap();

    let apic_base_virt = page_tables::translate_p2v(PhysAddr::new(apic_base_phys))
        .expect("Physical Address of local APIC is not mapped.").addr();

    serial!("apic_base: {:#x} -> {:#x}\n", apic_base_phys, apic_base_virt);

    let apic_base: *const u8 = core::ptr::with_exposed_provenance(apic_base_virt);

    unsafe {
        let id_register = apic_base.add(0x20).cast::<u32>().read();
        assert!(id_register == 0x00, "Expected BSP to have id 0");
    }

    const AP_TRAMPOLINE_PAGE_IDX: usize = 0x40; // 0x40th page, starting at 0x40000

    let tramp_page_addr = AP_TRAMPOLINE_PAGE_IDX << 12;

    serial!("__end_of_trampoline: {:#p}\n", &raw const __end_of_trampoline);
    serial!("ap_init address: {:#x}\n", ap_init as *const () as usize);

    // Make sure trampoline page is identity mapped.
    assert_eq!(
        tramp_page_addr,
        page_tables::translate_p2v(PhysAddr::new(tramp_page_addr))
            .expect("AP tampoline page is not mapped").addr(),
        "AP Trampoline page is not identity mapped."
    );

    // Ensure that the trampoline function code fits in one half of a page.
    let tramp_fn_begin_addr = sipi_trampoline as *const () as usize;
    let tramp_fn_end_addr = (&raw const __end_of_trampoline).addr();

    let tramp_code_size = tramp_fn_end_addr.strict_sub(tramp_fn_begin_addr);
    assert!(tramp_code_size < 0x1000/2, "Trampoline function should fit in \
        half a page.");

    let tramp_page = core::ptr::with_exposed_provenance_mut(tramp_page_addr);

    // Copy trampoline code to the trampoline page.
    unsafe {
        core::ptr::copy(sipi_trampoline as *const u8, tramp_page, 
            tramp_code_size);
    }

    // Boot remaining three cores from id 1-to-3
    for i in 1..2u32 { unsafe {
        // Set up trampoline stack.
        let tramp_stack = tramp_page.add(1<<12);
        tramp_stack.sub(0x04).cast::<usize>().write(cpu::read_cr3());
        tramp_stack.sub(0x08).cast::<usize>().write(isr::get_idtr_address().addr());
        tramp_stack.sub(0x0c).cast::<usize>().write(gdt::get_gdtr_address().addr());
        tramp_stack.sub(0x10).cast::<usize>().write(ap_init as *const () as usize);

        let icr0 = apic_base.add(0x300).cast::<u32>().cast_mut();
        let icr1 = apic_base.add(0x310).cast::<u32>().cast_mut();

        // Write target cpu lapic ID.
        icr1.write(i << 24);
        icr0.write(0x4500); // send IPI.

        utils::sleep_millis(200);
        icr0.write(0x4600 | AP_TRAMPOLINE_PAGE_IDX as u32);

        utils::sleep_millis(200);

        // cpu::disable_interrupts();
    }}

    // unsafe { cpu::disable_interrupts() };

    loop {}

    panic!("mp_init end");
}

unsafe extern "C" {
    static __end_of_trampoline: [u8; 0];
}

extern "cdecl" fn ap_init() {
    unsafe {
        serial!("awake and aware...\n");

        let esp: usize;
        asm!(
            "mov    eax, esp",
            out("eax") esp
        );

        serial!("esp: {:#x}\n", esp);

        // cpu::enable_interrupts();
        // utils::sleep_secs(2);
        // serial!("slept 2 secs\n");
    }
    loop {}
}

// Self modifying code ahead!!!
#[rustc_align(4096)]
#[unsafe(naked)]
pub extern "cdecl" fn sipi_trampoline(gdt: usize, idt: usize, cr3: usize) {
    core::arch::naked_asm!(r#"
        .code16

            cli
            cld

            // Set stack pointer to 0x41000 - 0x10
            movw    $0x4000, %ax
            movw    %ax, %ss
            movw    $0x1000, %ax
            sub     $0x10, %ax
            movw    %ax, %sp

            mov     $0x4000, %ax    // Segment of the 0x40000 address.
            mov     %ax, %ds
            lgdt    %ds:0x2c        // Offset of __tramp_gdtr from the page
                                    // beginning.

            // Enable protected mode.
            mov     %cr0, %eax
            or      $0x01, %eax
            mov     %eax, %cr0

            // The trampoline function is loaded at the page address 0x40000,
            // 0x58 is the offset of `protected_mode` from the beginning of this
            // page.

        .code32
            // Far jump to address 0x08:0x00040058
            .byte   0x66, 0xea, 0x58, 0x00, 0x04, 0x00, 0x08, 0x00

        .align 4
        __tramp_gdtr:
            .2byte      23
            .4byte      0x40040     // __tramp_gdt begins at offset 0x40 from
                                    // page beginning.

        .align 16
        __tramp_gdt:
            .8byte      0x0000000000000000  // Null
            .8byte      0x00cf9b000000ffff  // 32-bit code
            .8byte      0x00cf93000000ffff  // 32-bit data

        __protected_mode:
            cli
            cld

            mov     $0x10, %ax
            mov     %ax, %ds
            mov     %ax, %es
            mov     %ax, %fs
            mov     %ax, %gs
            mov     %ax, %ss

            // Set the stack to the end of the trampoline page.
            mov     $0x41000, %eax
            sub     $0x10, %eax
            mov     %eax, %esp

            // Set cr3
            mov     0x0c(%esp), %eax
            mov     %eax, %cr3

            // Set paging: PG and PE
            mov     %cr0, %eax
            or      $0x80000001, %eax
            mov     %eax, %cr0

            // Does not optimizes out ap_init function.
            mov     ${ap_init}, %eax
            jmp     *%eax
            ret

            .global __end_of_trampoline
            __end_of_trampoline:
        "#,

        ap_init = sym ap_init,
        options(att_syntax),
    );
}

