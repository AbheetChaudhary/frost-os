use core::arch::asm;

use crate::println;

pub fn print_segment_registers() {
    let cs: u16;
    let ss: u16;
    let ds: u16;
    let es: u16;
    let fs: u16;
    let gs: u16;

    unsafe {
        asm!(
            "mov {cs:x}, cs",
            "mov {ss:x}, ss",
            "mov {ds:x}, ds",
            "mov {es:x}, es",
            "mov {fs:x}, fs",
            "mov {gs:x}, gs",

            cs = out(reg) cs,
            ss = out(reg) ss,
            ds = out(reg) ds,
            es = out(reg) es,
            fs = out(reg) fs,
            gs = out(reg) gs,
        );
    }

    println!("cs = {:#06x}", cs);
    println!("ss = {:#06x}", ss);
    println!("ds = {:#06x}", ds);
    println!("es = {:#06x}", es);
    println!("fs = {:#06x}", fs);
    println!("gs = {:#06x}", gs);

    print_segment(cs);
    print_segment(ss);
    print_segment(ds);
    print_segment(es);
    print_segment(fs);
    print_segment(gs);
}

pub fn print_segment(seg: u16) {
    let idx = seg >> 3;
    let table = (seg >> 1) & 0x1;
    let rpl = seg & 0x3;

    println!("index: {}, table: {}, rpl: {}", idx,
        if table == 0 { "GDT"} else { "LDT" }, rpl);
}

pub fn read_msr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;

    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            lateout("eax") lo,
            lateout("edx") hi,
            options(nomem, nostack, preserves_flags),
        );
    }

    ((hi as u64) << 32) | lo as u64
}

// Ticks elapsed in 1 second. Value may vary with system.
// Use `sudo dmesg | grep tsc` to get it.
const SEC_TICKS: u64 = 1_190_400_000;

// Ticks elapsed in 1 millisecond.
const MILLISEC_TICKS: u64 = 1_190_400;

fn rdtsc() -> u64 {
    unsafe { core::arch::x86::_rdtsc() }
}

/// Do nothing for `secs` number of seconds.
pub fn sleep_secs(secs: u64) {
    let begin = rdtsc();
    let end = begin + (secs * SEC_TICKS);

    while rdtsc() < end {
        core::hint::black_box(());
    }
}

/// Do nothing for `millis` number of milliseconds.
pub fn sleep_millis(millis: u64) {
    let begin = rdtsc();
    let end = begin + (millis * MILLISEC_TICKS);

    while rdtsc() < end {
        core::hint::black_box(());
    }
}

