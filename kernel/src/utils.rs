use core::arch::asm;

use crate::{timer, serial};

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

    serial!("cs = {:#06x}\n", cs);
    serial!("ss = {:#06x}\n", ss);
    serial!("ds = {:#06x}\n", ds);
    serial!("es = {:#06x}\n", es);
    serial!("fs = {:#06x}\n", fs);
    serial!("gs = {:#06x}\n", gs);

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

    serial!("index: {}, table: {}, rpl: {}\n", idx,
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

// // Ticks elapsed in 1 second. Value may vary with system.
// // Use `sudo dmesg | grep tsc` to get it.
// const SEC_TICKS: u64 = 1_190_400_000;

// // Ticks elapsed in 1 millisecond.
// const MILLISEC_TICKS: u64 = 1_190_400;

// fn rdtsc() -> u64 {
//     unsafe { core::arch::x86::_rdtsc() }
// }

// pub fn sleep_secs(secs: usize) {
//     let begin = rdtsc();
//     let end = begin + (secs as u64 * SEC_TICKS);

//     while rdtsc() < end {
//         core::hint::black_box(());
//     }
// }

// pub fn sleep_millis(millis: usize) {
//     let begin = rdtsc();
//     let end = begin + (millis as u64 * MILLISEC_TICKS);

//     while rdtsc() < end {
//         core::hint::black_box(());
//     }
// }

/// Do nothing(spin) for `secs` number of seconds.
pub fn sleep_secs(secs: usize) {
    let current_ticks = timer::get_current_ticks();

    let end_ticks = match current_ticks.checked_add(timer::SYSTEM_TIMER_FREQ * secs) {
        Some(t) => t,
        None    => {
            panic!("trying to sleep for {} seconds will overflow the system tick count.",
                secs);
        }
    };

    // TODO: do it in a better way after implementing scheduler.
    while timer::get_current_ticks() < end_ticks {
        core::hint::spin_loop();
    }
}

/// Do nothing(spin) for `millis` number of milliseconds.
pub fn sleep_millis(millis: usize) {
    const MILLIS_PER_TICK: usize = 1000 / timer::SYSTEM_TIMER_FREQ;

    if MILLIS_PER_TICK < 1 {
        panic!("Trying to use a very high frequency timer?");
    }

    // Number of ticks we should wait for in order to let `millis` ms pass.
    //
    // For 100Hz system clock, there will be 10ms per tick. So any attempt to
    // sleep for less than 10ms will end up in a 10ms sleep.
    //
    // For sleep durations greater than 10ms, the number of ticks will be
    // rounded up. So 2 ticks for 15ms and 3 ticks for 21ms sleep durations.
    let delta_ticks = if millis < MILLIS_PER_TICK {
        1
    } else {
        millis.div_ceil(MILLIS_PER_TICK)
    };

    let current_ticks = timer::get_current_ticks();

    let end_ticks = match current_ticks.checked_add(delta_ticks) {
        Some(t) => t,
        None    => {
            panic!("trying to sleep for {} millis will overflow the system tick count.",
                millis);
        }
    };

    // TODO: do it in a better way after implementing scheduler.
    while timer::get_current_ticks() < end_ticks {
        core::hint::spin_loop();
    }
}

pub unsafe fn mem_dump(ptr: *const u8, len: usize) {
    serial!("\n#####################################################\n");
    serial!("Memory Dump: ptr = {ptr:p}, length: 0x{len:x}\n\n");

    let mut i = 0;

    let mut pairs_on_this_line = 0; // goes up to 8, then print a newline.

    while i + 2 <= len {
        if pairs_on_this_line == 8 {
            serial!("\n");
            pairs_on_this_line = 0;
        }

        if pairs_on_this_line > 0 {
            serial!(" ");
        }

        let b_lo = unsafe { ptr.add(i).read_volatile() };
        let b_hi = unsafe { ptr.add(i + 1).read_volatile() };

        serial!("{b_lo:02x}{b_hi:02x}");

        pairs_on_this_line += 1;

        i += 2;
    }

    // If true then there is only one byte remaining to print.
    if i < len {
        if pairs_on_this_line == 8 {
            serial!("\n");
        }

        let b_lo = unsafe { ptr.add(i).read_volatile() };

        serial!("{b_lo:0x}");
    }

    serial!("\n#####################################################\n\n");
}
