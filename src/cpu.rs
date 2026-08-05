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

