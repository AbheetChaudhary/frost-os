use core::mem::MaybeUninit;
use core::arch::asm;

/*
use crate::{
    elf,
    uefi,
    protocols,

    proto_call, proto_call_strict,
    bs_call_strict,
    serial, serialn,
    utf16,
};
*/

use crate::*;

use mm::{PhysAddr, VirtAddr, PhysAddrRange, PhysMemType, Address};

pub struct KernelFile {
    /// The pointer to kernel file data.
    data: PhysAddr,

    /// The size of kernel file.
    #[allow(unused)]
    size: usize,

    /// Virtual address at which kernel entry function exists.
    entry: VirtAddr,

    /// Read only loadable segment.
    rdonly: elf::Loadable,

    /// Executable loadable segment.
    exec: elf::Loadable,

    /// Read/Write loadable segment.
    read_write: elf::Loadable,
}

/// Load the kernel file in memory as `KernelFile`.
pub fn load_kernel_file(device: uefi::Handle) -> KernelFile {
    // Locate simple fs protocol on the device.
    let ptr = handle_protocol_strict(device, 
        &protocols::EFI_SIMPLE_FILE_SYSTEM_PROTOCOL_GUID);

    let fs_proto = ptr.cast::<protocols::SimpleFileSystemProtocol>();

    let fs_proto_revision = unsafe { (*fs_proto).revision };

    // Assert file protocol revision 1.0.
    assert!(fs_proto_revision == protocols::EFI_FILE_PROTOCOL_REVISION);
    serialn!("file system protocol revision: 0x{:x}", fs_proto_revision);

    // Uninitialized pointer to file protocol associated with device.
    let mut dev_file_proto: MaybeUninit<*mut protocols::FileProtocol> =
        MaybeUninit::zeroed();

    // Get the file protocol supported by the file system protocol.
    unsafe {
        proto_call_strict!(fs_proto.open_volume(dev_file_proto.as_mut_ptr()));
    }

    // Initialized file protocol interface associated with above device.
    let dev_file_proto = unsafe { dev_file_proto.assume_init() };

    let fp_revision = unsafe { (*dev_file_proto).revision };

    serialn!("file protocol revision: 0x{:x}", fp_revision);

    // Uninitialized pointer to the file protocol associated with the kernel
    // file.
    let mut kernel_file = MaybeUninit::<*mut protocols::FileProtocol>::zeroed();

    // Path at which kernel is to be found.
    let kernel_path = utf16!("\\EFI\\kernel.bin\0");

    // Opening the kernel file.
    let r = unsafe {
        proto_call!(dev_file_proto.open(kernel_file.as_mut_ptr(),
            kernel_path.as_ptr().cast(), 0x01, 0))
    };

    // Initialized kernel file protocol pointer.
    let kernel_file = unsafe { kernel_file.assume_init() };

    if r.is_not_success() {
        panic!("failed to open kernel file: {:?}", r);
    }

    serialn!("kernel file successfully opened");

    // Allocate a 512 bytes big pool to store kernel file info.
    let info_pool = unsafe { allocate_pool_strict(512) };
    let mut pool_size = 512;

    // Read file info in info_pool.
    unsafe {
        proto_call_strict!(kernel_file.get_info(&protocols::EFI_FILE_INFO_GUID,
                &mut pool_size, info_pool));
    };

    let file_info = info_pool.cast::<protocols::FileInfo>();

    // Dump kernel file to serial.
    unsafe {
        protocols::FileInfo::dump_to_serial(file_info);
    }

    // Since allocate_pool gives 8-byte aligned addresses, and file info has
    // been read at the beginning of info_pool, so the FileInfo is guaranteed to
    // be properly aligned.
    assert!(align_of::<protocols::FileInfo>() <= 8);

    // Size of the kernel. Use .physical_size instead of .file_size as
    // physical_size may be sometimes bigger than the actual file_size, thus has
    // more redundancy.
    let file_size = unsafe { (*file_info).physical_size };

    // Convert file size to usize, panic if file too big.
    let mut file_size: usize = file_size.try_into()
        .expect("kernel size greater than 4GB...!");

    // Allocate memory to hold the kernel binary.
    let kernel_bin_ptr = unsafe { allocate_pool_strict(file_size) };

    // Read kernel file into memory.
    unsafe {
        proto_call_strict!(kernel_file.read(&mut file_size, kernel_bin_ptr));
    }

    // After successful return of read(), file_size contains the actual number
    // of bytes read, i.e. the number of initialized bytes in the buffer pointed
    // by kernel_bin_ptr.

    let file = unsafe { 
        core::slice::from_raw_parts(kernel_bin_ptr.cast::<u8>(), file_size)
    };

    serialn!("successfully read file in memory!");

    let (loadables, kernel_entry_addr) = elf::get_loadables_and_entry(file).unwrap();

    for l in &loadables {
        serialn!("{:#?}", l);
    }
    serialn!("entry point: 0x{:x}", kernel_entry_addr);

    let rdonly = loadables.iter().find(|seg| seg.perm == 0b100)
        .expect("no segment in kernel binary with r-- permissions");

    let exec = loadables.iter().find(|seg| seg.perm == 0b101)
        .expect("no segment in kernel binary with r-x permissions");

    let rw = loadables.iter().find(|seg| seg.perm == 0b110)
        .expect("no segment in kernel binary with rw- permissions");

    let kernel_file = KernelFile {
        data: PhysAddr::new(kernel_bin_ptr.addr()),
        size: file_size,
        entry: VirtAddr::new(kernel_entry_addr),
        rdonly: *rdonly,
        exec: *exec,
        read_write: *rw,
    };

    unsafe {
        bs_call_strict!(free_pool(file_info.cast()));
        // bs_call_strict!(free_pool(kernel_bin_ptr));
    }

    kernel_file
}

impl KernelFile {
    pub fn start_kernel(&self, mut phys_mem: mm::PhysMem) -> ! {
        // Already have the initial physical memory description.

        init_segment(&mut phys_mem, self.data, &self.exec)
            .expect("loading of exec segment should have succeeded");

        init_segment(&mut phys_mem, self.data, &self.rdonly)
            .expect("loading of rdonly segment should have succeeded");

        init_segment(&mut phys_mem, self.data, &self.read_write)
            .expect("loading of read/write segment should have succeeded");

        let cr3 = create_virtual_address_map(&mut phys_mem, self)
            .expect("Should have created the virtual memory map");

        // TODO: find loader stack among boot services data and map only just
        // that region.

        unsafe { enable_32bit_paging(cr3.addr()) };

        kserial!("page table initialized, jumping to kernel entry...\n");

        let kernel_entry: unsafe extern "Rust" fn(mm::PhysMem) -> ! = unsafe {
            core::mem::transmute(self.entry)
        };

        unsafe {
            kernel_entry(phys_mem);
        }
    }
}

/// Get the memory map from firmware and conver it to PhysMem.
pub fn prepare_for_memory_takeover() -> Result<mm::PhysMem, mm::Error> {
    // New empty physical memory map.
    let mut mem_uninit = mm::PhysMem::new_empty();

    let address_map = get_memory_map_strict();

    // Memory region types that need preserving after our memory takeover.
    #[allow(non_snake_case)]
    let NEEDS_PRESERVING: [uefi::MemoryType; 10] = {
        use uefi::MemoryTypeBefore::*;

        [
            AcpiMemoryNvs.into(), AcpiReclaimMemory.into(),
            LoaderCode.into(), LoaderData.into(),
            BootServicesCode.into(), BootServicesData.into(),
            ReservedMemoryType.into(), RuntimeServicesCode.into(),
            RuntimeServicesData.into(),
            ConventionalMemory.into(),
        ]
    };

    // Number of memory descriptors get_memory_map_strict provided.
    let map_desc_count = address_map.map_size / address_map.desc_size;

    for i in 0..map_desc_count {
        let ptr = address_map.desc_ptr.map_addr(|addr| addr + i * address_map.desc_size);

        let desc = unsafe { ptr.read() };

        // If it is not one of the memory regions that needs preserving then
        // skip it.
        if !NEEDS_PRESERVING.contains(&desc.memory_type) {
            continue;
        }

        // Memory type of this region after our takeover.
        let pmt = mem_type_efi_to_boot(desc.memory_type)
            .ok_or(mm::Error::InvalidMemType)?;

        // Convert from u64 to usize.
        let region_begin: usize = desc.physical_start.0
            .try_into().map_err(|_| mm::Error::BigIntError)?;

        // Convert from u64 to usize.
        let num_pages: usize = desc.num_pages.try_into()
            .map_err(|_| mm::Error::BigIntError)?;

        let region_end: usize = region_begin
            .strict_add(num_pages.strict_mul(1<<12) - 1);

        let region_addr_range = PhysAddrRange::new(
            PhysAddr::new(region_begin),
            PhysAddr::new(region_end),
        ).map_err(|_| mm::Error::InvalidAddrRange)?;

        let region = mm::PhysMemRegion::new(region_addr_range, pmt)
            .map_err(|_| mm::Error::InvalidRegion)?;

        unsafe { mem_uninit.register(region)? }
    }

    // Registered are needed regions.
    let mem_init = mem_uninit;

    Ok(mem_init)
}

/// Converts from Efi memory type to our PhysMemType.
fn mem_type_efi_to_boot(efi_mem: uefi::MemoryType) -> Option<PhysMemType> {
    use uefi::EfiMemoryType as EMT;
    use PhysMemType as PMT;

    let efi_mem: EMT = efi_mem.try_into().ok()?;

    let pmt = match efi_mem {
        EMT::ACPIMemoryNVS => PMT::AcpiMemoryNvs,
        EMT::ACPIReclaimMemory => PMT::AcpiReclaimMemory,

        EMT::RuntimeServicesCode => PMT::RuntimeServicesCode,
        EMT::RuntimeServicesData => PMT::RuntimeServicesData,

        EMT::ConventionalMemory => PMT::UnusedMemory,

        EMT::LoaderCode => PMT::LoaderCode,
        EMT::LoaderData => PMT::LoaderData,

        EMT::BootServicesCode => PMT::BootServicesCode,
        EMT::BootServicesData => PMT::BootServicesData,

        EMT::ReservedMemoryType => PMT::ReservedMemoryType,

        // Other memory types are currently unused as they do not show up in
        // OVMF's memory map. But they could be easily handled.
        _ => return None,
    };

    Some(pmt)
}

/// Create a physical memory region for the loadable kernel segment.
///
/// It takes the address to which kernel file has been loaded and the loadable
/// segment description and carves out a physical memory region.
fn init_segment(phys_mem: &mut mm::PhysMem, data: PhysAddr, segment: &elf::Loadable)
    -> Result<(), mm::Error>
{
    // Size of the segment in memory.
    let mem_size = segment.mem_sz;

    // Number of pages in memory this segment will need.
    // let page_count = mem_size.div_ceil(1 << 12);

    let page_count = {
        let p_vaddr_low = segment.virt_addr.addr() & !((1<<12) - 1);

        let p_vaddr_end = segment.virt_addr.addr().strict_add(mem_size);

        let p_vaddr_high = if p_vaddr_end % (1<<12) > 0 {
            (p_vaddr_end.strict_add(1<<12)) & !((1<<12) - 1)
        } else {
            p_vaddr_end
        };

        p_vaddr_high.strict_sub(p_vaddr_low) / (1<<12)
    };

    let segment_type = match segment.perm {
        0b100 => PhysMemType::KernelRodata,
        0b101 => PhysMemType::KernelCode,
        0b110 => PhysMemType::KernelData,

        _ => return Err(mm::Error::InvalidMemType),
    };

    // Allocate new region to hold this segment.
    let segment_region = phys_mem.allocate_region(page_count, segment_type)?;

    // Copy segment data to this new region.

    // Beginning of the segment data.
    let src_begin = data.checked_add(segment.offset)
        .ok_or(mm::Error::IntegerOverflow)?;

    // The segment contents might start with an offset in the virtual address
    // map.
    //
    // Like if the executable segment wants to be loaded at 0xc0001140. Here the
    // segment skips first 0x140 bytes of the page.
    let dst_begin_offset = {
        let p_vaddr = segment.virt_addr.addr();

        p_vaddr - (p_vaddr & !((1<<12) - 1))
    };

    for i in 0..dst_begin_offset {
        unsafe {
            segment_region.first().checked_add(i)
                .ok_or(mm::Error::IntegerOverflow)?.write::<u8>(0);
        }
    }

    // Beginning of the segment destination region.
    let dst_begin = segment_region.first().checked_add(dst_begin_offset)
        .ok_or(mm::Error::IntegerOverflow)?;

    // Copy file_sz bytes. The actual number of bytes from the kernel binary
    // data.
    for i in 0..segment.file_sz {
        unsafe {
            let b = src_begin.checked_add(i).ok_or(mm::Error::IntegerOverflow)?.read::<u8>();
            dst_begin.checked_add(i).ok_or(mm::Error::IntegerOverflow)?.write::<u8>(b);
        }
    }

    let slack_bytes_count = {
        // Count of remaining uninitialized bytes.
        //
        // We allocated some pages, zero initialized first dst_begin_offset
        // bytes of it, then copied segment.file_sz bytes.
        page_count.strict_mul(1<<12) - dst_begin_offset - segment.file_sz
    };

    // If there is any slack, then overwrite it with zero.
    if slack_bytes_count > 0 {
        let slack_dst_begin = dst_begin.checked_add(segment.file_sz)
            .ok_or(mm::Error::IntegerOverflow)?;

        for i in 0..slack_bytes_count {
            unsafe {
                slack_dst_begin.checked_add(i)
                    .ok_or(mm::Error::IntegerOverflow)?.write::<u8>(0);
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PagingError {
    /// Virtual address is already occupied.
    VirtAddrOccupied,

    FrameAllocError,

    /// Integer error specifically related to u32/u64/usize conversions.
    BigIntError,

    UnalignedAddr,

    IntegerOverflow,

    Unmapped,

    PhysMemError(mm::Error),
}

impl From<mm::Error> for PagingError {
    fn from(other: mm::Error) -> Self {
        Self::PhysMemError(other)
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PageDirEntry(u32);

impl PageDirEntry {
    fn is_empty(&self) -> bool {
        self.0 == 0
    }

    fn new(pde: u32) -> PageDirEntry {
        Self(pde)
    }

    fn pt_addr(&self) -> Option<PhysAddr> {
        if self.is_empty() {
            return None;
        }

        let pde = self.0;

        let pt_addr = pde & (u32::MAX << 12);

        let pt_addr_usize: usize = pt_addr.try_into().unwrap();

        Some(PhysAddr::new(pt_addr_usize))
    }
}

#[repr(C, align(4096))]
pub struct PageDirectory {
    pd_entries: [PageDirEntry; 1024],
}

impl PageDirectory {
    unsafe fn insert_mapping(&mut self, virt: VirtAddr, phys: PhysAddr,
        frame_alloc: &mut FrameAlloc) -> Result<(), PagingError> {

        if phys.addr() % (1<<12) != 0 ||
            virt.addr() % (1<<12) != 0 {

            return Err(PagingError::UnalignedAddr);
        }

        let pde_idx = (virt.addr() >> 22) & ((1<<10) - 1);

        // Allocate a new page table if pde is empty.
        if self.pd_entries[pde_idx].is_empty() {
            // Allocate a new page for storing a PageTable.
            let pt_addr = frame_alloc.allocate_frame()
                .map_err(|_| PagingError::FrameAllocError)?;

            let pt_addr_u32: u32 = pt_addr.addr().try_into()
                .map_err(|_| PagingError::BigIntError)?;

            // read/write, present.
            let pde = (pt_addr_u32 & !((1<<12) - 1)) | (1<<1) | (1<<0);

            self.pd_entries[pde_idx] = PageDirEntry::new(pde);
        }

        // Address of the page table referenced by PDE at pde_idx.
        let pt_addr = self.pd_entries[pde_idx].pt_addr()
            .expect("we have already filled it.");

        let pt = unsafe { pt_addr.as_mut::<PageTable>() };

        unsafe { pt.insert_mapping(virt, phys)? };

        Ok(())
    }

    #[allow(unused)]
    unsafe fn translate(&self, virt: VirtAddr)
        -> Result<PhysAddr, PagingError>
    {
        let pde_idx = (virt.addr() >> 22) & ((1<<10) - 1);

        // If PDE does not exist then return err.
        if self.pd_entries[pde_idx].is_empty() {
            return Err(PagingError::Unmapped);
        }

        // Address of the page table referenced by PDE at pde_idx.
        let pt_addr = self.pd_entries[pde_idx].pt_addr()
            .expect("already checked that the PDE mapping exists.");

        let pt = unsafe { pt_addr.as_ref::<PageTable>() };

        unsafe { pt.translate(virt) }
    }

    unsafe fn _remove_mapping(&mut self, _virt: VirtAddr)
        -> Result<(), PagingError> {

        todo!();
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PageTableEntry(u32);

impl PageTableEntry {
    fn is_empty(&self) -> bool {
        self.0 == 0
    }

    fn new(pte: u32) -> PageTableEntry {
        Self(pte)
    }
}

#[repr(C, align(4096))]
pub struct PageTable {
    pt_entries: [PageTableEntry; 1024],
}

impl PageTable {
    unsafe fn insert_mapping(&mut self, virt: VirtAddr, phys: PhysAddr)
        -> Result<(), PagingError> {

        let pte_idx = (virt.addr() >> 12) & ((1<<10) - 1);

        if !self.pt_entries[pte_idx].is_empty() {
            return Err(PagingError::VirtAddrOccupied);
        }

        // Physical address of the page referenced by this PTE.
        let page_addr_u32: u32 = phys.addr().try_into()
            .map_err(|_| PagingError::BigIntError)?;

        // read/write, present.
        let pte = (page_addr_u32 & !((1<<12) - 1)) | (1<<1) | (1<<0);

        self.pt_entries[pte_idx] = PageTableEntry::new(pte);

        Ok(())
    }

    #[allow(unused)]
    unsafe fn translate(&self, virt: VirtAddr)
        -> Result<PhysAddr, PagingError> {

        let pte_idx = (virt.addr() >> 12) & ((1<<10) - 1);

        if self.pt_entries[pte_idx].is_empty() {
            return Err(PagingError::Unmapped);
        }

        let offset = virt.addr() & ((1<<12) - 1);

        let page_addr_u32 = self.pt_entries[pte_idx].0;
        let page_addr = page_addr_u32 & !((1<<12) - 1);

        let page_addr: usize = page_addr.try_into().unwrap();

        let translated_addr = page_addr + offset;

        Ok(PhysAddr::new(translated_addr))
    }

    unsafe fn _remove_mapping(&mut self, _virt: VirtAddr)
        -> Result<(), PagingError> {

        todo!();
    }
}

/*
fn inspect(pd: &PageDirectory, addr: VirtAddr) {
    kserial!("Virtual({:#x}) --> ", addr.addr());
    kserial!("Physical({:#x})\n",
        unsafe { pd.translate(addr).expect("translation failed").addr() });
}

fn inspect_region(pd: &PageDirectory, region: &mm::PhysMemRegion) {
    for i in 0..region.page_count() {
        let begin = region.first().strict_add(i.strict_mul(1<<12));
        inspect(pd, begin.as_virt_addr());
    }
}
*/

unsafe fn enable_32bit_paging(cr3: usize) {
    // CR4.PSE = 0, PDE PS flag = 0

    let x: usize = 0;
    kserial!("stack contains address: {:p}\n", &x);

    kserial!("Setting CR3 to {:#x}\n", cr3);
    // Set cr3 and enable PG and PE bits of cr0.
    unsafe {
        asm!(r#"
                mov     cr3, eax

                mov     eax, cr0
                or      eax, 0x80000001
                mov     cr0, eax
            "#,

            in("eax") cr3,
        );
    }

}

/// Takes in the completed physical memory region description, complete with all
/// the preservable regions and kernel segments loaded, and creates virtual
/// address map, returning the value that will go in CR3.
pub fn create_virtual_address_map(phys_mem: &mut mm::PhysMem, kernel_file: &KernelFile)
    -> Result<PhysAddr, PagingError> {

    // Physical memory regions of type:
    // - AcpiMemoryNvs
    // - AcpiReclaimMemory
    // - RuntimeServicesCode
    // - RuntimeServicesData
    // ...will get arbitrary virtual addresses.
    // 
    // Memory regions of type:
    // - LoaderCode
    // - LoaderData
    // - BootServicesCode
    // - BootServicesData
    // ...will get identity mapped.
    //
    // Memory regions of type:
    // - KernelCode
    // - KernelData
    // - KernelRodata
    // ...will get virtual addresses from `kernel`.
    //
    // BiosRegion and ReservedMemoryType will not get any virtual mapping.

    // Allocate memory to hold a single page directory. The origin PD.
    let origin_pde_region = phys_mem.allocate_region(1, PhysMemType::LoaderData)?;

    // Address at which the origin PD will be located.
    let origin_pde_begin_addr = origin_pde_region.first();

    // Fill the page directory with all zeroes.
    for i in 0..1024 {
        unsafe {
            origin_pde_begin_addr
                .checked_add(i * size_of::<u32>())
                .ok_or(mm::Error::IntegerOverflow)?
                .write::<u32>(0);
        }
    }

    // Create a mutable reference to the origin PD.
    let origin_pd = unsafe {
        origin_pde_begin_addr.as_mut::<PageDirectory>()
    };

    // Virtual address at which we will start mapping things.
    //
    // Large enough that no identity maps of loader code/data will conflict with
    // other mappings.
    const VIRT_START: usize = 32 * 0x100000; // 8MiB

    let mut virt = VirtAddr::new(VIRT_START);

    // Allocate memory to allocate PTEs. We need only 1024 pages for the PT's.
    // The origin page for the PD is already there.
    let pt_region = phys_mem.allocate_region(1024, PhysMemType::LoaderData)?;

    let mut frame_alloc = unsafe {
        FrameAlloc::new(pt_region).map_err(|_| PagingError::FrameAllocError)?
    };

    // Virtual map AcpiMemoryNvs and AcpiReclaimMemory.
    virt = map_region(&phys_mem, origin_pd, virt, PhysMemType::AcpiMemoryNvs, 
        &mut frame_alloc)?;
    virt = map_region(&phys_mem, origin_pd, virt, 
        PhysMemType::AcpiReclaimMemory, &mut frame_alloc)?;

    kserial!("done acpi virtual map\n");

    // Virtual map runtime services code and data.
    virt = map_region(&phys_mem, origin_pd, virt,
        PhysMemType::RuntimeServicesCode, &mut frame_alloc)?;
    map_region(&phys_mem, origin_pd, virt,
        PhysMemType::RuntimeServicesData, &mut frame_alloc)?;

    kserial!("done runtime services virtual map\n");

    // Identity map loader code and data.
    identity_map_region(&phys_mem, origin_pd, PhysMemType::LoaderCode,
        &mut frame_alloc)?;
    identity_map_region(&phys_mem, origin_pd, PhysMemType::LoaderData,
        &mut frame_alloc)?;

    kserial!("done loader code/data virtual map\n");

    identity_map_region(&phys_mem, origin_pd, PhysMemType::BootServicesCode,
        &mut frame_alloc).unwrap();
    identity_map_region(&phys_mem, origin_pd, PhysMemType::BootServicesData,
        &mut frame_alloc).unwrap();
    kserial!("done boot services code/data virtual map\n");

    // Inspect loader code.
    /*
    for r in phys_mem.active_regions() {
        if r.mem_type() == PhysMemType::LoaderCode {
            kserial!("-----inspecting region(loader code)-----\n");
            inspect_region(origin_pd, r);
        }
    }
    */

    // Inspect loader data.
    /*
    for r in phys_mem.active_regions() {
        if r.mem_type() == PhysMemType::LoaderData {
            kserial!("-----inspecting region(loader data)-----\n");
            inspect_region(origin_pd, r);
        }
    }
    */


    // Map kernel code.
    let k_code_virt = kernel_file.exec.virt_addr;
    let k_code_virt = VirtAddr::new(k_code_virt.addr() & !((1<<12) - 1));
    kserial!("exec page: {:#x}\n", k_code_virt.addr());
    map_region(&phys_mem, origin_pd, k_code_virt, PhysMemType::KernelCode, 
        &mut frame_alloc)?;

    // Map kernel r/w data.
    let k_rw_virt = kernel_file.read_write.virt_addr;
    let k_rw_virt = VirtAddr::new(k_rw_virt.addr() & !((1<<12) - 1));
    kserial!("rw page: {:#x}\n", k_rw_virt.addr());
    map_region(&phys_mem, origin_pd, k_rw_virt, PhysMemType::KernelData, 
        &mut frame_alloc)?;

    // Map kernel rodata.
    let k_rodata_virt = kernel_file.rdonly.virt_addr;
    let k_rodata_virt = VirtAddr::new(k_rodata_virt.addr() & !((1<<12) - 1));
    kserial!("rodata page: {:#x}\n", k_rodata_virt.addr());
    map_region(&phys_mem, origin_pd, k_rodata_virt, PhysMemType::KernelRodata, 
        &mut frame_alloc)?;

    // TODO: map kernel stack.

    Ok(origin_pde_begin_addr)
}

fn identity_map_region(
    phys_mem: &mm::PhysMem,
    pd: &mut PageDirectory,
    mem_type: PhysMemType,
    frame_alloc: &mut FrameAlloc,
) -> Result<(), PagingError> {

    for region in phys_mem.active_regions() {
        // If region is of our concerned type then map it page by page.
        if region.mem_type() == mem_type {
            let region_begin = region.first();

            let mut virt = region_begin.as_virt_addr();

            for _ in 0..region.page_count() {
                unsafe {
                    pd.insert_mapping(virt, virt.as_phys_addr(), frame_alloc)?; 
                }

                // inspect(pd, virt);

                virt = virt.checked_add(1<<12)
                    .ok_or(PagingError::IntegerOverflow)?;
            }
        }
    }

    Ok(())
}

fn map_region(
    phys_mem: &mm::PhysMem,
    pd: &mut PageDirectory,
    start_virt: VirtAddr,
    mem_type: PhysMemType,
    frame_alloc: &mut FrameAlloc,
) -> Result<VirtAddr, PagingError> {

    let mut virt =  start_virt;

    for region in phys_mem.active_regions() {
        // If region is of our concerned type then map it page by page.
        if region.mem_type() == mem_type {
            let region_begin = region.first();

            for page_idx in 0..region.page_count() {
                unsafe {
                    pd.insert_mapping(
                        virt,
                        region_begin.checked_add(page_idx.strict_mul(1<<12))
                            .ok_or(PagingError::IntegerOverflow)?,
                        frame_alloc,
                    )?; 
                }

                virt = virt.checked_add(1<<12)
                    .ok_or(PagingError::IntegerOverflow)?;
            }
        }
    }

    Ok(virt)
}
        

union FrameLink {
    link: usize,
    _junk: MaybeUninit<[u8; size_of::<usize>()]>,
}

pub struct FrameAlloc {
    region: mm::PhysMemRegion,

    /// Index of the next free frame. If it is equal to the frame count of the
    /// region then there are no more free frames to allocate.
    next: usize,

    used_frames: usize,
}

impl FrameAlloc {
    pub unsafe fn new(region: mm::PhysMemRegion) -> Result<FrameAlloc, ()> {
        let page_count = region.page_count();

        for i in 0..page_count {
            // Beginning of the next frame.
            let frame_begin = region.first()
                .checked_add(i.checked_mul(1<<12).ok_or(())?).ok_or(())?;

            let link = FrameLink { link: i.checked_add(1).ok_or(())? };

            unsafe {
                frame_begin.write::<FrameLink>(link);
            }
        }

        let fa = Self {
            region,
            next: 0,
            used_frames: 0,
        };

        Ok(fa)
    }

    pub fn allocate_frame(&mut self) -> Result<PhysAddr, ()> {
        if self.next == self.region.page_count() {
            return Err(());
        }

        let next_frame_begin = self.region.first()
            .checked_add(self.next.checked_mul(1<<12).ok_or(())?).ok_or(())?;

        let link = unsafe {
            next_frame_begin.read::<FrameLink>()
        };

        unsafe {
            self.next = link.link;
        }

        // Zero out the frame.
        for i in 0..(1<<12) {
            unsafe {
                next_frame_begin.checked_add(i).ok_or(())?.write::<u8>(0);
            }
        }

        self.used_frames += 1;

        Ok(next_frame_begin)
    }

    pub unsafe fn deallocate_frame(&mut self, frame_begin: PhysAddr) -> Result<(), ()> {
        if frame_begin.addr() % (1<<12) != 0 {
            return Err(());
        }

        if !self.region.contains(frame_begin) {
            return Err(());
        }

        let link = FrameLink { link: self.next };

        // Index of the to be deallocated frame in the regions.
        let frame_idx = (frame_begin.addr() - self.region.first_addr()) / (1<<12);

        unsafe {
            frame_begin.write::<FrameLink>(link);
        }

        self.next = frame_idx;
        self.used_frames -= 1;

        Ok(())
    }

    pub unsafe fn destroy(self) {
        drop(self)
    }
}
