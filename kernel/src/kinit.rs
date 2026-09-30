use core::{
    arch::asm,
    sync::atomic::{AtomicUsize, AtomicU32, Ordering},
};

use alloc::{
    vec::Vec,
};

use crate::{
    acpi,
    address,
    boot_mem::{self as bmm, Address},
    cpu,
    gdt,
    isr,
    kmalloc::CpuHeap,
    mm,
    page_tables,
    serial,
    utils,
    kmain,
};

use address::{
    PhysAddr32,
    PhysAddr64,
    PhysAddr,
    VirtAddr
};

use bmm::PhysMemType as bmt; // Boot mem types.
use mm::PhysMemType as kmt;  // Kernel mem types.

type KPhysMem = [Option<mm::PhysMemRegion>; MAX_K_REGION_COUNT];

/// This is the maximum stack size. The actual stack size might be smaller than
/// this.
///
/// This is due to the fact that the stack takes its pages from the KernelStack
/// memory region, which also provides the frames to store the page table during
/// mapping the stack.
const STACK_SIZE: usize = 128 * 1024; // 128KB.

const STACK_PAGE_COUNT: usize = STACK_SIZE / (1<<12);

/// Number of regions that could be there in the kernel description of the
/// physical memory.
const MAX_K_REGION_COUNT: usize = 256;

/// Virtual address at which the kernel stack will begin.
///
/// This is where the MOAT-STACK-MOAT region begins.
pub const KERNEL_STACK_BEGIN: usize = 8 * 1024 * 1024;

/// Size of the kernel stack.
pub const KERNEL_STACK_SIZE: usize = 128 * 1024; // 128KB

/// Unmapped virtual address space memory that will go before and after the
/// kernel stack and act as trap if accessed.
pub const KERNEL_STACK_MOAT: usize = KERNEL_STACK_SIZE / 2; // 64KB before and
                                                            // after the stack.

/// The physical frame index at which the trampoline code exists. This page is
/// identity mapped.
///
/// TODO: Remove this mapping after initialization.
pub const AP_TRAMPOLINE_PAGE_IDX: usize = 0x40; // 0x40th page, starting at 0x40000

/// Total virtual address space size reserved by the stack+heap.
///
/// The entire reserved address range might not be mapped, but the mapped
/// regions will not exceed this.
///
/// It is 4MB up aligned.
pub static STACK_HEAP_SIZE: AtomicUsize = AtomicUsize::new(0);

/// Whether the given boot memory type is reclaimable by the kernel to use as
/// stack or heap.
fn is_reclaimable_memory(mem_type: bmm::PhysMemType) -> bool {
    use bmm::PhysMemType::*;

    mem_type == UnusedMemory ||
        mem_type == BootServicesCode || 
        mem_type == BootServicesData ||
        mem_type == LoaderCode ||
        mem_type == LoaderData ||
        mem_type == LoaderStack
}

// Memory types that are trivially translateable between bmm and kmm.
const TRIVIAL_TRANSLATE_TABLE: [(bmt, kmt); 10] = [
    (bmt::AcpiMemoryNvs, kmt::AcpiMemoryNvs),
    (bmt::AcpiReclaimMemory, kmt::AcpiReclaimMemory),

    (bmt::RuntimeServicesCode, kmt::RuntimeServicesCode),
    (bmt::RuntimeServicesData, kmt::RuntimeServicesData),

    (bmt::KernelCode, kmt::KernelCode),
    (bmt::KernelData, kmt::KernelData),
    (bmt::KernelRodata, kmt::KernelRodata),

    (bmt::PagingData, kmt::PagingData), // TODO: Should this be the way to
                                        // handle paging data frames?

    (bmt::BiosRegion, kmt::BiosRegion),

    (bmt::ReservedMemoryType, kmt::ReservedMemoryType),
];

/// A function to translate boot time memory types to kernel memory types.
///
/// It only works for types that are trivially translateable.
fn trivial_bmt_to_kmt(mem_type: bmm::PhysMemType) -> Option<mm::PhysMemType> {
    use bmm::PhysMemType as bmt; // Boot mem types.
    use mm::PhysMemType as kmt;  // Kernel mem types.

    for i in 0..TRIVIAL_TRANSLATE_TABLE.len() {
        if TRIVIAL_TRANSLATE_TABLE[i].0 == mem_type {
            return Some(TRIVIAL_TRANSLATE_TABLE[i].1);
        }
    }

    None
}

/// Convert between boot time to kernel time representation of physical memory.
///
/// The bootloader is not as complicated as the kernel and does not needs to
/// have as detailed view of the physical memory as the kernel. So the simple
/// boot time description needs translation to make it suitable for kernel
/// usage.
pub fn translate_phys_mem_boot_to_kernel(mut boot_phys_mem: bmm::PhysMem)
    -> [Option<mm::PhysMemRegion>; MAX_K_REGION_COUNT] {

    // Number of cores to boot.
    let core_count: u32 = 4;

    // Number of unused pages of memory.
    let mut unused_memory_pages: usize = 0;

    boot_phys_mem.active_regions().for_each(|r| {
        if is_reclaimable_memory(r.mem_type()) {
            unused_memory_pages += r.page_count();
        }
    });

    // Usable memory.
    let total_usable_pages: usize = unused_memory_pages;
    serial!("[BSP] total usable memory pages: {:#x} pages\n", total_usable_pages);

    // Total stack memory for all cores.
    let total_stack_pages: usize = core_count as usize * STACK_PAGE_COUNT;
    serial!("[BSP] total stack pages: {:#x} pages\n", total_stack_pages);

    // Total memory available for heap after taking out stack pages.
    let total_heap_pages = total_usable_pages - total_stack_pages;
    serial!("[BSP] usable heap pages: {:#x} pages\n", total_heap_pages);

    // Per core heap memory pages.
    let per_core_heap_pages = total_heap_pages / (core_count as usize + 1);
    serial!("[BSP] per core heap pages: {per_core_heap_pages:#x} pages\n");

    // Size taken by kernel heap and stack.
    let mut stack_heap_size = KERNEL_STACK_SIZE
        .strict_add(KERNEL_STACK_MOAT.strict_mul(2))
        .strict_add(per_core_heap_pages << 12);

    // 4MB up align the size.
    if stack_heap_size % (1<<22) != 0 {
        stack_heap_size &= !((1<<22) - 1);
        stack_heap_size = stack_heap_size.strict_add(1<<22);
    }

    // The virtual address of the stack and heap should not conflict with the
    // kernel code and data.
    assert!(KERNEL_STACK_BEGIN 
        + core_count as usize * stack_heap_size < 0xc0000000);

    STACK_HEAP_SIZE.store(stack_heap_size, Ordering::Relaxed);

    // Available shared heap pages.
    //
    // NOTE:
    //
    // All the unused regions after building kernel stack, per core heap, etc.
    // will become part of the shared heap pages. Ideally bmt::PagingData should
    // also be repurposed to KernelSharedHeap but is currently not being done
    // so.

    let origin_pd = unsafe { page_tables::get_origin_pd_mut() };

    // Unmap all the regions of LoaderCode and LoaderData. These two regions are
    // identity mapped by the bootloader.
    for r in boot_phys_mem.active_regions() {
        if r.mem_type() == bmt::LoaderCode || r.mem_type() == bmt::LoaderData {
            let begin = PhysAddr::new(r.first().addr());
            let page_count = r.page_count();

            for i in 0..page_count {
                let phys = begin.strict_add(i<<12);
                let virt = VirtAddr::new(phys.addr());

                let ret = unsafe { origin_pd.delete_mapping(virt) };

                match ret {
                    Ok(p) => {
                        assert!(p == phys);
                    }

                    Err(e) => {
                        panic!("failed to unmape LoaderCode/LoaderData: {e:?}");
                    }
                }
            }
        }
    }
    serial!("[BSP] unmapped LoaderCode/Data pages\n");

    // Reclaim memory from boot physical memory description.
    //
    // Mark each reclaimable memory region as unused memory.
    for boot_region in boot_phys_mem.active_regions_mut() {
        if is_reclaimable_memory(boot_region.mem_type()) {
            unsafe { boot_region.set_mem_type(bmm::PhysMemType::UnusedMemory) };
        }
    }

    let mut k_phys_mem: [Option<mm::PhysMemRegion>; MAX_K_REGION_COUNT] = [None; _];

    // Number of used regions in k_phys_mem.
    let mut k_region_count = 0;

    for boot_region in boot_phys_mem.active_regions() {
        let mem_type = boot_region.mem_type();

        use bmm::PhysMemType::*;

        // After reclaiming, memory type of the region must be one of these.
        let after_reclaim_types: [_; _] = [
            AcpiMemoryNvs,
            AcpiReclaimMemory,

            RuntimeServicesCode,
            RuntimeServicesData,

            UnusedMemory,

            PagingData,

            KernelCode,
            KernelStack,
            KernelData,
            KernelRodata,

            BiosRegion,

            ReservedMemoryType,
        ];

        assert!(after_reclaim_types.contains(&mem_type));
    }

    // Simple closure to convert boot physical address range to kernel physical
    // address range.
    let boot_to_kernel_range = |b_range: bmm::PhysAddrRange| {
        address::PhysRange::from_start_and_last(
            PhysAddr::new(b_range.first_addr()),
            PhysAddr::new(b_range.last_addr())
        )
    };

    // Copy trivially translateable regions to k_phys_mem.
    for r in boot_phys_mem.regions.iter_mut() {
        if r.is_none() {
            continue;
        }

        let boot_region = r.as_ref().unwrap();

        if let Some(k_mem_type) = trivial_bmt_to_kmt(boot_region.mem_type()) {
            let addr_range = boot_to_kernel_range(boot_region.addr_range());

            let k_mem_region = mm::PhysMemRegion::new(addr_range, k_mem_type)
                .unwrap();

            k_phys_mem[k_region_count] = Some(k_mem_region);
            k_region_count += 1;

            // Delete this region, it has been absorbed by k_phys_mem.
            *r = None;
        }
    }

    // Repurpose bmt::KernelStack as kmt::KernelStack(LapicId(0)).
    //
    // NOTE: This assumes that LapicId(0) is the BSP.
    for r in boot_phys_mem.regions.iter_mut() {
        if r.is_none() {
            continue;
        }

        let boot_region = r.as_mut().unwrap();

        if boot_region.mem_type() == bmt::KernelStack {
            let k_region = mm::PhysMemRegion::new(
                boot_to_kernel_range(boot_region.addr_range()),
                kmt::KernelStack(mm::LapicId(0)),
            ).unwrap();

            k_phys_mem[k_region_count] = Some(k_region);
            k_region_count += 1;

            // Delete the boot region after absorbing it in kernel.
            *r = None;
        }
    }

    // Create regions for per core stack.
    'outer: for core_id in 1..core_count {
        // Collect the stack regions for this core.

        // Number of stack pages that we have found for core_id. This is less
        // than or equal to STACK_PAGE_COUNT.
        let mut stack_pages_found: usize = 0;

        'inner: for r in boot_phys_mem.regions.iter_mut() {
            if r.is_none() {
                continue;
            }

            let boot_region = r.as_mut().unwrap();

            if boot_region.mem_type != bmt::UnusedMemory {
                continue;
            }

            let pages_needed = STACK_PAGE_COUNT.strict_sub(stack_pages_found);

            let k_region;

            // If all the pages in the boot_region could be used then consume
            // the entire region.
            if boot_region.page_count() <= pages_needed {
                // The entire region could be used to provide stack pages.
                k_region = mm::PhysMemRegion::new(
                    boot_to_kernel_range(boot_region.addr_range()),
                    kmt::KernelStack(mm::LapicId(core_id)),
                ).unwrap();

                stack_pages_found += boot_region.page_count();

                // Delete the region after absorbing it completely.
                *r = None;
            } else {
                // We only need a part of this region to complete the kernel
                // stack region.

                // Number of pages we need to take from this region.
                let pages_needed = STACK_PAGE_COUNT.strict_sub(stack_pages_found);

                assert!(pages_needed < boot_region.page_count());

                let carved_range = boot_region.take_from_beginning(pages_needed)
                    .expect("already checked that we are not taking all the pages \
                        from the region");

                k_region = mm::PhysMemRegion::new(
                    boot_to_kernel_range(carved_range.addr_range()),
                    kmt::KernelStack(mm::LapicId(core_id)),
                ).unwrap();

                stack_pages_found += pages_needed;
            }

            k_phys_mem[k_region_count] = Some(k_region);
            k_region_count += 1;

            if stack_pages_found == STACK_PAGE_COUNT {
                break 'inner;
            }
        }
    }

    // Create regions for per core heap.
    'outer: for core_id in 0..core_count {
        // Collect the heap regions for this core.

        // Number of heap pages that we have found for core_id. This is less
        // than or equal to per_core_heap_pages.
        let mut heap_pages_found: usize = 0;

        'inner: for r in boot_phys_mem.regions.iter_mut() {
            if r.is_none() {
                continue;
            }

            let boot_region = r.as_mut().unwrap();

            if boot_region.mem_type != bmt::UnusedMemory {
                continue;
            }

            let pages_needed = per_core_heap_pages.strict_sub(heap_pages_found);

            let k_region;

            // If all the pages in the boot_region could be used then consume
            // the entire region.
            if boot_region.page_count() <= pages_needed {
                // The entire region could be used to provide stack pages.
                k_region = mm::PhysMemRegion::new(
                    boot_to_kernel_range(boot_region.addr_range()),
                    kmt::KernelHeap(mm::LapicId(core_id)),
                ).unwrap();

                heap_pages_found += boot_region.page_count();

                // Delete the region after absorbing it completely.
                *r = None;
            } else {
                // We only need a part of this region to complete the kernel
                // core heap region.

                // Number of pages we need to take from this region.
                let pages_needed = per_core_heap_pages.strict_sub(heap_pages_found);

                assert!(pages_needed < boot_region.page_count());

                let carved_range = boot_region.take_from_beginning(pages_needed)
                    .expect("already checked that we are not taking all the pages \
                        from the region");

                k_region = mm::PhysMemRegion::new(
                    boot_to_kernel_range(carved_range.addr_range()),
                    kmt::KernelHeap(mm::LapicId(core_id)),
                ).unwrap();

                heap_pages_found += pages_needed;
            }

            k_phys_mem[k_region_count] = Some(k_region);
            k_region_count += 1;

            if heap_pages_found == per_core_heap_pages {
                break 'inner;
            }
        }
    }

    // Repurpose unused region into KernelSharedHeap.
    for r in boot_phys_mem.regions.iter_mut() {
        if r.is_none() {
            continue;
        }

        let boot_region = r.as_mut().unwrap();

        let mt = boot_region.mem_type();

        if mt == bmt::UnusedMemory {
            let k_region = mm::PhysMemRegion::new(
                boot_to_kernel_range(boot_region.addr_range()),
                kmt::KernelSharedHeap,
            ).unwrap();

            k_phys_mem[k_region_count] = Some(k_region);
            k_region_count += 1;

            *r = None;
        }
    }

    let active_boot_regions_count = boot_phys_mem.active_regions().count();
    assert!(active_boot_regions_count == 0);

    // Shared heap page frames.
    let mut shared_heap_pages_count = 0;
    for r in k_phys_mem.iter() {
        if r.is_none() {
            continue;
        }

        let r = r.as_ref().unwrap();

        if r.mem_type() == kmt::KernelSharedHeap {
            shared_heap_pages_count += r.page_count();
        }
    }

    serial!("[BSP] shared_heap_pages: {:#x}\n", shared_heap_pages_count);

    k_phys_mem
}

fn map_heap_pages(k_phys_mem: &[Option<mm::PhysMemRegion>], lapic_id: mm::LapicId)
    -> (*mut u8, usize) {

    // Count of the number of pages available in region KernelHeap(LapicId...).
    let mut page_count = 0;

    // The start of the first frame in the kernel heap region.
    let mut lowest_frame_start = usize::MAX;

    for r in k_phys_mem.iter() {
        if r.is_none() {
            continue;
        }

        let region = r.as_ref().unwrap();

        if region.mem_type() == kmt::KernelHeap(lapic_id) {
            page_count += region.page_count();
            lowest_frame_start = lowest_frame_start.min(region.start_addr());
        }
    }

    serial!("[Core#{}] KernelHeap({lapic_id:?}) page_count: {page_count}, \
        first frame at: {lowest_frame_start:#x}\n", lapic_id.0);

    /*
    let info_pages = page_count.div_ceil(1<<12);
    serial!("info pages count: {info_pages}\n");
    */

    // The stack is surrounded by equal size moat.
    //
    // [----MOAT----][----STACK----][----MOAT----]
    // ^
    // |
    // KERNEL_STACK_BEGIN
    let heap_begin = KERNEL_STACK_BEGIN
        .strict_add(STACK_HEAP_SIZE.load(Ordering::Relaxed)
            .strict_mul(lapic_id.0 as usize))
        .strict_add(KERNEL_STACK_MOAT.strict_mul(2))
        .strict_add(KERNEL_STACK_SIZE);

    assert!(heap_begin % (1<<12) == 0);

    serial!("[Core#{}] heap_begin: {:#x}\n", lapic_id.0, heap_begin);

    // Virtual address at which this core's heap will begin.
    let heap_virt_begin = VirtAddr::new(heap_begin);

    // Number of pages that have been mapped to heap.
    //
    // This number might be less than the page_count available for this core's
    // heap, this is because some of those heap pages might be used allocate
    // page tables themselves.
    let mut mapped_heap_pages = 0;

    // Number of pages that were used to build the pageing data structures
    // themselves. The frames used for this are also taken from kernel heap
    // region.
    let mut mapped_pt_pages = 0;

    let origin_pd = unsafe { page_tables::get_origin_pd_mut() };

    // An iterator over the regions of type KernelHeap(LapicId...).
    let heap_regions_iter = k_phys_mem.iter().filter_map(|r| {
        r.as_ref()
            .filter(|r| r.mem_type() == kmt::KernelHeap(lapic_id))
            .copied()
    });

    // Flattening the heap_regions_iter to get an iterator of physical frames
    // that make up the region KernelHeap(LapicId...).
    let mut heap_frames_iter = heap_regions_iter.flat_map(|r| {
        r.frame_iter()
    });

    // This is the number of pages from KernelHeap region that were neither used
    // for the heap and nor for the paging structures.
    //
    // While trying to map a heap page, we might need to map an additional page
    // to store the page table, that itself might need an additional page, etc.
    // If any of these fail because we ran out of frames from KernelHeap region
    // then all these page table mappings and one heap mapping would have to be
    // cancelled and they all will become the wasted pages.
    //
    // There might be some upper bound over how bad this could get but I haven't
    // calculated it yet.
    let mut wasted_pages = 0;

    loop {
        // Get a new page from the frame iterator that will be used as a heap
        // page.
        let heap_page = match heap_frames_iter.next() {
            Some(p) => p,
            None => {
                serial!("[Core#{}] all heap region frames successfully mapped.\n",
                    lapic_id.0);
                break;
            }
        };

        let phys = heap_page;
        let virt = heap_virt_begin.strict_add(mapped_heap_pages << 12);

        let ret = unsafe {
            origin_pd.insert_mapping_with_frame_iter(virt, phys,
                &mut heap_frames_iter)
        };

        match ret {
            Ok(n) => {
                // Successfully mapped 1 heap page, that consumed n extra frames
                // for creating the page tables.

                mapped_heap_pages += 1;
                mapped_pt_pages += n;
            }

            Err((e, n)) => {
                // Failure could happen due to multiple reasons, but we only
                // care about one, i.e., if mapping of the heap page failed due
                // to running out of pages to use for the page table itself.
                //
                // If this happens then it should happen in the end of mapping
                // the heap, when most of the pages have been successfully
                // mapped but we ran out of frames to write the final page
                // tables of some final heap pages.

                // Assert that the iterator ran out.
                serial!("[Core#{}] virt: {:#x}, phys: {:#x}\n", lapic_id.0,
                    virt.addr(), phys.addr());
                serial!("[Core#{}] heap_pages: {mapped_heap_pages}, pt_pages: \
                    {mapped_pt_pages}, n + 1: {}, e: {e:?}\n", lapic_id.0, n + 1);
                assert!(heap_frames_iter.next().is_none());

                // All the mapped heap and PT pages, 1 heap page that we tried
                // to map but failed, and n wasted pages that we wasted while
                // trying to create the page tables.
                assert!(mapped_heap_pages + mapped_pt_pages + 1 + n == page_count);

                // 100 and 5 are just randomly chosen numbers. Page count is
                // some big number and n is some small number.
                assert!(page_count > 100);
                assert!(n + 1 < 5);

                // If wasted pages is less than 5 then that's good enough.
                wasted_pages += (1 + n);
                break;
            }
        }
    }

    serial!("[Core#{}] total heap frames: {page_count}, heap pages: \
        {mapped_heap_pages}, pt pages: {mapped_pt_pages}\n", lapic_id.0);

    // Everything from heap_begin to up to mapped_heap_pages number of pages is
    // now mapped.
    let heap_begin: *mut u8 = core::ptr::
        with_exposed_provenance_mut(heap_virt_begin.addr());

    let heap_size: usize = mapped_heap_pages.strict_mul(1<<12);

    (heap_begin, heap_size)
}

/// Map stack pages for the core with `lapic` ID.
///
/// Returns the lowest mapped stack address(page aligned) and the number of
/// stack pages mapped.
fn map_stack_pages(k_phys_mem: &[Option<mm::PhysMemRegion>],
    lapic: mm::LapicId) -> (VirtAddr, usize) {

    // Assuming BSP is 0.
    assert!(lapic.0 != 0);

    let stack_heap_size = STACK_HEAP_SIZE.load(Ordering::Relaxed);

    // Stack begins after a moat.
    let ap_stack_begin = KERNEL_STACK_BEGIN
        .strict_add(stack_heap_size.strict_mul(lapic.0 as usize))
        .strict_add(KERNEL_STACK_MOAT);

    let ap_stack_begin_virt = VirtAddr::new(ap_stack_begin);

    // Number of pages of kernel stack region that were used for stack.
    let mut mapped_stack_pages = 0;

    // Number of pages of kernel stack region that were used for page tables
    // mapping for the stack.
    let mut mapped_pt_pages = 0;

    // Number of pages of the kernel stack region that were neither used for
    // stack nor for page table mappings.
    //
    // This could happen when trying to map a stack page that requires some PT
    // pages, but the iterator ran out of pages. So the stack page and any PT
    // page that was used before running out will now be a wasted page.
    //
    // This should be at most 3(arbitrarily chosen).
    let mut wasted_pages = 0;

    // An iterator over regions of type KernelStack(LapicId...).
    let stack_region_iter = k_phys_mem.iter().filter_map(|r| {
        r.as_ref()
            .filter(|r| r.mem_type() == kmt::KernelStack(lapic))
            .copied()
    });

    // An iterator over the physical frames available in all regions of type
    // KernelStack(LapicId...).
    let mut stack_frame_iter = stack_region_iter.flat_map(|r| {
        r.frame_iter()
    });

    let page_count = stack_frame_iter.clone().count();

    let origin_pd = unsafe { page_tables::get_origin_pd_mut() };

    loop {
        let stack_frame = match stack_frame_iter.next() {
            Some(f) => f,
            None => {
                break;
            }
        };

        let phys = stack_frame;
        let virt = ap_stack_begin_virt.strict_add(mapped_stack_pages<<12);

        let ret = unsafe {
            origin_pd.insert_mapping_with_frame_iter(virt, phys,
                &mut stack_frame_iter)
        };

        match ret {
            Ok(n) => {
                // serial!("{:#x} => {:#x}\n", virt.addr(), phys.addr());

                mapped_stack_pages += 1;
                mapped_pt_pages += n;
            }

            Err((e, n)) => {
                // This error must have happened only because of running out of
                // frames from the kernel stack regions.
                assert!(matches!(e, page_tables::PagingError::FrameAllocError));

                // Assert that the iterator ran out.
                assert!(stack_frame_iter.next().is_none());

                // All the mapped stack and PT pages, 1 stack page that we tried
                // to map but failed, and n wasted pages that we wasted while
                // trying to create the page tables.
                assert!(mapped_stack_pages + mapped_pt_pages + 1 + n == page_count);

                // If wasted pages is less than 5 then that's good enough.
                wasted_pages += (1 + n);
                serial!("[{lapic:?}] wasted_pages: {wasted_pages}\n");
                assert!(wasted_pages < 4);

                break;
            }
        }
    }

    (ap_stack_begin_virt, mapped_stack_pages)
}

pub fn init_all_cores(boot_phys_mem: bmm::PhysMem) -> ! {
    use mm::LapicId;

    // Translate between boot time physical memory representation to kernel time
    // representation.
    let k_phys_mem = translate_phys_mem_boot_to_kernel(boot_phys_mem);

    let (apic_base, tramp_stack) = mp_setup();

    let core_count = 4u32;
    kmain::WAITING_CORES.store(core_count, Ordering::Relaxed);

    // Initialize the stacks of other AP's.
    for i in 1..core_count {
        let (s_begin, sp_count) = map_stack_pages(&k_phys_mem, LapicId(i));

        // Top of the empty stack.
        let mut s_top = s_begin.strict_add(sp_count << 12);

        assert!(sp_count != 0);

        s_top = s_top.strict_sub(size_of::<KPhysMem>());
        s_top = s_top.map_addr(|a| a & !(align_of::<KPhysMem>() - 1));

        unsafe {
            s_top.as_mut::<KPhysMem>().write(k_phys_mem)
        }

        if s_top.addr() % 16 != 0 {
            s_top = s_top.map_addr(|a| a & !(16 - 1));
        }

        // Initialize the AP stack.
        unsafe {
            // Dead space of 4 bytes to make sure that stack pointer is 16 bytes
            // aligned before entry.
            s_top = s_top.strict_sub(4);

            s_top = s_top.strict_sub(4);
            s_top.as_mut::<u32>().write(i);

            s_top = s_top.strict_sub(4);
            s_top.as_mut::<*const KPhysMem>().write(core::ptr::from_ref(&k_phys_mem));

            s_top = s_top.strict_sub(4);
            s_top.as_mut::<*const u8>().write(cpu::halt as *const u8);

            assert!(s_top.addr() % 16 == 0);

            // ---------------
            // |   GARBAGE   |
            // ---------------
            // |  lapic_id   |
            // ---------------
            // |  &KPhysMem  |
            // ---------------
            // |RET -> halt()|   <-  Stack top for ap_init
            // ---------------
        }

        // Initialize the trampoline stack.


        unsafe {
            tramp_stack.sub(0x04).cast::<usize>().write(s_top.addr());
            tramp_stack.sub(0x08).cast::<usize>().write(cpu::read_cr3());

            let icr0 = apic_base.add(0x300).cast::<u32>().cast_mut();
            let icr1 = apic_base.add(0x310).cast::<u32>().cast_mut();

            // Write target cpu lapic ID.
            icr1.write(i << 24);
            icr0.write(0x4500); // send IPI.

            utils::sleep_millis(10);
            icr0.write(0x4600 | AP_TRAMPOLINE_PAGE_IDX as u32);

            utils::sleep_millis(20);

            // cpu::disable_interrupts();
        }

    }

    // Assuming BSP id is 0.
    unsafe { ap_init(&k_phys_mem, 0) };
}

/// Does some basic sanity checks and returns the virtual address at which the
/// APIC base is mapped and the top of the empty trampoline stack.
///
/// The top of the empty trampoline stack is actually the end of the tampoline
/// page. This function also checks that the trampoline code fits within half a
/// page, and is page aligned. The later half of the page is used as the stack.
///
/// Self modifying code ahead!!!
fn mp_setup(/* madt_ptr: *const acpi::RawMadt */) -> (*const u8, *mut u8) {
    /*
    assert!(cpu::is_modern_ia_processor(), "Not a modern processor! Don't \
        know how to boot multiple cores");
    */

    // let madt_ref = unsafe { acpi::Madt::from_raw_madt_ptr(madt_ptr) };

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
    let tramp_fn_begin_addr = __sipi_trampoline as *const () as usize;
    let tramp_fn_end_addr = (&raw const __end_of_trampoline).addr();

    assert!(tramp_fn_begin_addr % (1<<12) == 0, 
        "Trampoline code is not page aligned");

    let tramp_code_size = tramp_fn_end_addr.strict_sub(tramp_fn_begin_addr);
    assert!(tramp_code_size < 0x1000/2, "Trampoline function should fit in \
        half a page.");

    let tramp_page = core::ptr::with_exposed_provenance_mut(tramp_page_addr);

    // Copy trampoline code to the trampoline page.
    unsafe {
        core::ptr::copy(__sipi_trampoline as *const u8, tramp_page, 
            tramp_code_size);
    }

    let tramp_stack = unsafe { tramp_page.add(1<<12) };
    (apic_base, tramp_stack)
}

unsafe extern "cdecl" fn ap_init(k_phys_mem: &[Option<mm::PhysMemRegion>; MAX_K_REGION_COUNT], 
    lapic_id: u32) -> ! {

    // Initialize the heap of the AP core.
    let lapic_id = mm::LapicId(lapic_id);
    let (heap_begin, heap_size) = map_heap_pages(k_phys_mem.as_slice(), lapic_id);

    let heap = unsafe { CpuHeap::init(heap_begin, heap_size) };

    unsafe { kmain::kmain(&heap, lapic_id); }
}

unsafe extern "C" {
    static __end_of_trampoline: [u8; 0];
}

#[rustc_align(4096)]
#[unsafe(naked)]
extern "cdecl" fn __sipi_trampoline(cr3: usize, esp: usize) {
    core::arch::naked_asm!(r#"
        .code16

            cli
            cld

            // Set stack pointer to 0x41000 - 0x10
            movw    $0x4000, %ax
            movw    %ax, %ss
            movw    $0x1000, %ax

            // This could have been a 0x08 as well. But lets just pretend that
            // we have a return address on the stack as well.
            //
            // esp, cr3, <return address>
            sub     $0x0c, %ax
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
            sub     $0x0c, %eax
            mov     %eax, %esp

            // Load new stack address in ecx
            mov     0x08(%esp), %ecx

            // Set cr3
            mov     0x04(%esp), %eax
            mov     %eax, %cr3

            // Set paging: PG and PE
            mov     %cr0, %eax
            or      $0x80000001, %eax
            mov     %eax, %cr0

            // Update the stack.
            mov     %ecx, %esp

            // Does not optimizes out ap_init function.
            mov     ${ap_init}, %eax
            jmp     *%eax

            ud2

            .global __end_of_trampoline
            __end_of_trampoline:
        "#,

        ap_init = sym ap_init,
        options(att_syntax),
    );
}

