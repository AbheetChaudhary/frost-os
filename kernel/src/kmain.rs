extern crate alloc;

use alloc::{
    vec::Vec,
};

use core::{
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    serial,
    cpu,
    mm,
    kmalloc::CpuHeap,
};

/// Number of cores that are waiting for release in `kmain(...)`.
pub static WAITING_CORES: AtomicU32 = AtomicU32::new(0);

pub unsafe fn kmain(heap: &CpuHeap, lapic_id: mm::LapicId) -> ! {
    // If this is the last core to reach kmain then print this message.
    if WAITING_CORES.load(Ordering::Relaxed) == 1 {
        serial!("\nReleasing all cores...\n\n");
    }

    WAITING_CORES.fetch_sub(1, Ordering::Release);

    while WAITING_CORES.load(Ordering::Acquire) != 0 {
        core::hint::spin_loop();
    }

    serial!("[Core#{}] awake and aware...\n", lapic_id.0);

    let mut v: Vec<i32, &CpuHeap> = Vec::with_capacity_in(4 * 1024 * 1024, heap);

    for _ in 0..32 {
        v.push(lapic_id.0 as i32);
        core::hint::black_box(&mut v);
    }

    while let Some(_) = v.pop() {}
    drop(v);

    cpu::forever_spin();
}

