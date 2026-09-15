use core::arch::asm;

#[inline(always)]
pub unsafe fn disable_interrupts() {
    unsafe {
        asm!(
            "cli"
        );
    }
}

#[inline(always)]
pub unsafe fn enable_interrupts() {
    unsafe {
        asm!(
            "sti"
        );
    }
}

/// Checks whether the current CPU has CPUID instruction available.
pub fn has_cpuid() -> bool {
    let answer: u32;

    // CPUID bit is at 22nd youngest bit.
    const CPUID_MASK: u32 = 1 << 21;

    unsafe {
        asm!(
            "pushfd",

            "pushfd",
            "pop        eax",
            "mov        ecx, eax",
            "and        ecx, {cpuid_mask}",
            "xor        eax, {cpuid_mask}",
            "push       eax",
            "popfd",
            "pushfd",
            "pop        eax",
            "and        eax, {cpuid_mask}",
            "cmp        eax, ecx",
            "setne      al",

            "popfd",

            out("eax") answer,
            out("ecx") _,
            cpuid_mask = const CPUID_MASK,
        )
    }

    (answer & 0xff) == 1
}

/// Checks whether extended CPUID is there or not.
///
/// SAFETY: Must ensure that CPUID is there.
pub unsafe fn has_extended_cpuid() -> bool {
    let answer: u32;

    unsafe {
        asm!(
            "cpuid",

            inout("eax") 0x80000000u32 => answer,

            out("ebx") _,
            out("ecx") _,
            out("edx") _,
        )
    }

    answer > 0x80000000
}

/// Checks whether long mode is supported.
///
/// SAFETY: Must ensure that CPUID is there.
pub unsafe fn has_long_mode() -> bool {
    let edx: u32;

    unsafe {
        asm!(
            "cpuid",

            inout("eax") 0x80000001u32 => _,
            out("ebx") _,
            out("ecx") _,

            out("edx") edx,
        )
    }

    (edx & (1 << 29)) != 0
}

pub fn read_cr0() -> usize {
    let cr0;
    unsafe {
        asm!(
            "mov {0}, cr0",

            out(reg) cr0
        )
    }

    cr0
}

pub fn read_cr2() -> usize {
    let cr2;
    unsafe {
        asm!(
            "mov {0}, cr2",

            out(reg) cr2
        )
    }

    cr2
}

pub fn read_cr3() -> usize {
    let cr3;
    unsafe {
        asm!(
            "mov {0}, cr3",

            out(reg) cr3
        )
    }

    cr3
}

// `qemu-system-i386 --machine pc-q35-10.0` gives family=6, model=6, extended model=0.
pub fn is_modern_ia_processor() -> bool {
    use core::arch::x86::__cpuid;
    use crate::serial;

    let ret = __cpuid(1);

    let eax = ret.eax;

    let family = (eax & 0x0000_0f00) >> 8;
    let extended_model = (eax & 0x000f_0000) >> 16;
    let model = (eax & 0xf0) >> 4;

    serial!("family: {}, model: {}, extended_model: {}\n", family,
        model, extended_model);

    family > 0x06 && extended_model > 0x00

}

pub unsafe fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;

    unsafe {
        asm!(
            "rdmsr",

            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
        );
    }

    let lo: u64 = lo.into();
    let hi: u64 = hi.into();

    (hi << 32) | lo
}
