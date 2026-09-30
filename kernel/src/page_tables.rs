use core::arch::asm;

use crate::{address, mm, as_cast, cpu};

use address::{PhysAddr, VirtAddr};
use mm::{FrameAllocator};

/// Virtual address at which serialized pages from the page table data
/// structures will exist.
///
/// CR3 contains a physical address 
pub const PAGE_TABLE_START: VirtAddr = VirtAddr::new(0xcf000000);

pub unsafe fn get_origin_pd<'a>() -> &'a PageDirectory {
    let addr = PAGE_TABLE_START.addr();
    let ptr = core::ptr::with_exposed_provenance(addr);

    unsafe { &*ptr }
}

pub unsafe fn get_origin_pd_mut<'a>() -> &'a mut PageDirectory {
    let addr = PAGE_TABLE_START.addr();
    let ptr = core::ptr::with_exposed_provenance_mut(addr);

    unsafe { &mut *ptr }
}

/// Decomposes a virtual address into the PDE index, PTE index and the
/// offset.
fn decompose(virt: VirtAddr) -> (usize, usize, usize) {
    let addr = virt.addr();

    let pde_idx = addr >> 22;
    let pte_idx = (addr >> 12) & ((1<<10) - 1);
    let offset = addr & ((1<<12) - 1);

    (pde_idx, pte_idx, offset)
}

pub fn translate_p2v(phys: PhysAddr) -> Option<VirtAddr> {
    // let cr3 = crate::cpu::read_cr3();

    // This is the address at which the page table structures begin in the
    // virtual address space.
    let cr3_mapped = PAGE_TABLE_START.addr();

    let pd_ptr: *const PageDirectory = core::ptr::with_exposed_provenance(cr3_mapped);

    let pd = unsafe { &*pd_ptr };

    let virt = pd.translate_phys_to_virt(phys);

    virt
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PagingError {
    /// Virtual address is already occupied.
    VirtAddrOccupied,

    FrameAllocError,

    UnalignedAddr,

    MissingPde,

    MissingPte,
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PageDirEntry(u32);

use PageDirEntry as PDE;

impl PageDirEntry {
    fn is_empty(&self) -> bool {
        self.0 == 0
    }

    fn new(pde: u32) -> PageDirEntry {
        Self(pde)
    }

    fn new_empty() -> PageDirEntry {
        Self::new(0)
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
    pub unsafe fn insert_mapping<F: FrameAllocator>(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        frame_alloc: &mut F
    ) -> Result<(), PagingError> {
        if phys.addr() % (1<<12) != 0 ||
            virt.addr() % (1<<12) != 0 {

            return Err(PagingError::UnalignedAddr);
        }

        let pde_idx = (virt.addr() >> 22) & ((1<<10) - 1);

        // If the PDE does not point to a page then return err.
        if self.pd_entries[pde_idx].is_empty() {
            // Allocate a new page for storing a PageTable.
            let pt_phys_addr = frame_alloc.allocate_frame()
                .map_err(|_| PagingError::FrameAllocError)?;

            let pt_phys_addr_u32 = as_cast!(pt_phys_addr.addr(), usize => u32);

            // read/write, present.
            let pde = (pt_phys_addr_u32 & !((1<<12) - 1)) | (1<<1) | (1<<0);

            // TODO: in case of failures we need to delete this entry.
            self.pd_entries[pde_idx] = PageDirEntry::new(pde);

            // Map this new page to appropriate virtual address.
            let pt_virt_addr = PAGE_TABLE_START.strict_add((1 + pde_idx) << 12);

            // TODO: How bad could this recursion get...?
            unsafe {
                self.insert_mapping(pt_virt_addr, pt_phys_addr, frame_alloc)?;
            }
        }

        // Address of the page table referenced by PDE at pde_idx.
        let pt_phys_addr = self.pd_entries[pde_idx].pt_addr()
            .expect("we have already filled it.");

        // let pt = unsafe { pt_addr.as_mut::<PageTable>() };
        let pt_virt_addr = PAGE_TABLE_START.strict_add((1 + pde_idx) << 12);

        let pt: *mut PageTable = unsafe {
            core::ptr::with_exposed_provenance_mut(pt_virt_addr.addr())
        };

        unsafe { (&mut *pt).insert_mapping(virt, phys)? };

        Ok(())
    }

    pub unsafe fn insert_mapping_with_frame_iter<I: Iterator<Item=PhysAddr>>(
        &mut self,
        virt: VirtAddr,
        phys: PhysAddr,
        frame_iter: &mut I,
    ) -> Result<usize, (PagingError, usize)> {
        if phys.addr() % (1<<12) != 0 ||
            virt.addr() % (1<<12) != 0 {

            return Err((PagingError::UnalignedAddr, 0));
        }

        let pde_idx = (virt.addr() >> 22) & ((1<<10) - 1);

        // Frames consumed from the iterator.
        let mut iter_consumed = 0;

        // If the PDE does not point to a page then return err.
        if self.pd_entries[pde_idx].is_empty() {
            // Allocate a new page for storing a PageTable.
            let pt_phys_addr = frame_iter.next()
                .ok_or((PagingError::FrameAllocError, 0))?;

            iter_consumed += 1;

            let pt_phys_addr_u32 = as_cast!(pt_phys_addr.addr(), usize => u32);

            // read/write, present.
            let pde = (pt_phys_addr_u32 & !((1<<12) - 1)) | (1<<1) | (1<<0);

            // TODO: in case of failures we need to delete this entry.
            self.pd_entries[pde_idx] = PageDirEntry::new(pde);

            // Map this new page to appropriate virtual address.
            let pt_virt_addr = PAGE_TABLE_START.strict_add((1 + pde_idx) << 12);

            // TODO: How bad could this recursion get...?
            unsafe {
                match self.insert_mapping_with_frame_iter(pt_virt_addr,
                    pt_phys_addr, frame_iter)
                {
                    Ok(n) => {
                        iter_consumed += n;

                        // Zero initialize the page table.
                        let pt: &mut PageTable = unsafe {
                            let ptr = core::ptr::
                                with_exposed_provenance_mut(pt_virt_addr.addr());

                            &mut *ptr
                        };

                        for i in 0..1024 {
                            pt.pt_entries[i] = PageTableEntry::new(0);
                        }
                    },
                    Err((e, n)) => {
                        iter_consumed += n;

                        return Err((e, iter_consumed));
                    }
                }
            }
        }

        // Address of the page table referenced by PDE at pde_idx.
        let pt_phys_addr = self.pd_entries[pde_idx].pt_addr()
            .expect("we have already filled it.");

        // let pt = unsafe { pt_addr.as_mut::<PageTable>() };
        let pt_virt_addr = PAGE_TABLE_START.strict_add((1 + pde_idx) << 12);

        let pt: *mut PageTable = unsafe {
            core::ptr::with_exposed_provenance_mut(pt_virt_addr.addr())
        };

        unsafe { (&mut *pt).insert_mapping(virt, phys)
            .map_err(|e| (e, iter_consumed))? };

        Ok(iter_consumed)
    }

    /// Delete the mapping of `virt` frame address if it is mapped, returning
    /// the physical address it was mapped to in `Ok`.
    ///
    /// Return `Err` if `virt` is currently not mapped or if it is not frame
    /// aligned.
    ///
    /// It runs `invlpg` in case it is going to return `Ok`.
    ///
    /// # SAFETY
    ///
    /// - It must be okay to delete the mapping.
    pub unsafe fn delete_mapping(&mut self, virt: VirtAddr)
        -> Result<PhysAddr, PagingError> {

        if !virt.is_aligned_to(1<<12) {
            return Err(PagingError::UnalignedAddr);
        }

        let (pde_idx, pte_idx, offset) = decompose(virt);

        if self.pd_entries[pde_idx].pt_addr().is_none() {
            return Err(PagingError::MissingPde);
        }

        let pt_virt_addr = PAGE_TABLE_START.strict_add((1 + pde_idx) << 12);

        let pt = unsafe {
            let pt_ptr: *mut PageTable =
                core::ptr::with_exposed_provenance_mut(pt_virt_addr.addr());

            &mut *pt_ptr
        };

        let frame = pt.pt_entries[pte_idx].frame_addr()
            .ok_or(PagingError::MissingPte)?;

        // Invalidate the TLB entry.
        unsafe {
            asm!(
                "invlpg     [{}]",

                in(reg) virt.addr(),
            );
        }

        // Clear the old page table entry.
        pt.pt_entries[pte_idx] = PageTableEntry::new_empty();

        Ok(frame)
    }

    pub fn translate_phys_to_virt(&self, phys: PhysAddr) -> Option<VirtAddr> {
        for pde_idx in 0..self.pd_entries.len() {
            if self.pd_entries[pde_idx].is_empty() {
                continue;
            }

            let pt_virt_addr = PAGE_TABLE_START.strict_add((1 + pde_idx) << 12);

            let pt = unsafe {
                let pt_ptr: *const PageTable =
                    core::ptr::with_exposed_provenance(pt_virt_addr.addr());

                &*pt_ptr
            };

            if let Some(pt_idx) = pt.find_indirection_pte(phys) {
                let pt_idx: usize = pt_idx.try_into().unwrap();
                let offset = phys.addr() & ((1<<12) - 1);
                let virt_addr = (pde_idx << 22) | (pt_idx << 12) | offset;
                return Some(VirtAddr::new(virt_addr));
            }
        }

        None
    }

    pub fn translate_virt_to_phys(&self, virt: VirtAddr) -> Option<PhysAddr> {
        let pde_idx = (virt.addr() >> 22) & ((1<<10) - 1);

        if self.pd_entries[pde_idx].is_empty() {
            return None;
        }

        let pt_virt_addr = PAGE_TABLE_START.strict_add((1 + pde_idx) << 12);

        let pt = unsafe {
            let pt_ptr: *const PageTable =
                core::ptr::with_exposed_provenance(pt_virt_addr.addr());

            &*pt_ptr
        };

        pt.translate_virt_to_phys(virt)
    }
}

#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct PageTableEntry(u32);

use PageTableEntry as PTE;

impl PageTableEntry {
    fn is_empty(&self) -> bool {
        self.0 == 0
    }

    fn new(pte: u32) -> PageTableEntry {
        Self(pte)
    }

    fn new_empty() -> PageTableEntry {
        Self::new(0)
    }

    fn frame_addr(&self) -> Option<PhysAddr> {
        if self.is_empty() {
            return None;
        }

        let pte = self.0;

        let frame_addr = pte & (u32::MAX << 12);

        let frame_addr_usize: usize = frame_addr.try_into().unwrap();

        Some(PhysAddr::new(frame_addr_usize))
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
            crate::serial!("virt: {:#x}, phys: {:#x}, phys existing: {:#x}\n", 
                virt.addr(), phys.addr(), self.pt_entries[pte_idx].frame_addr().unwrap().addr());
            return Err(PagingError::VirtAddrOccupied);
        }

        // Physical address of the page referenced by this PTE.
        let page_addr_u32: u32 = as_cast!(phys.addr(), usize => u32);

        // read/write, present.
        let pte = (page_addr_u32 & !((1<<12) - 1)) | (1<<1) | (1<<0);

        self.pt_entries[pte_idx] = PageTableEntry::new(pte);

        Ok(())
    }

    /// Get the page table entry that points to the `phys` physical address.
    pub fn find_indirection_pte(&self, phys: PhysAddr) -> Option<u32> {
        // Look for the page table entry with phys_frame_addr frame address.
        let phys_frame_addr = phys.addr() & !((1<<12) - 1);
        let phys_frame_addr: u32 = phys_frame_addr.try_into().unwrap();

        for (i, pte) in self.pt_entries.iter().enumerate() {
            if pte.is_empty() {
                continue;
            }

            let pte_u32 = pte.0 & !((1<<12) - 1);

            if pte_u32 == phys_frame_addr {
                return Some(i.try_into().unwrap())
            }
        }

        None
    }

    pub fn translate_virt_to_phys(&self, virt: VirtAddr) -> Option<PhysAddr> {
        let pte_idx = (virt.addr() >> 12) & ((1<<10) - 1);

        let offset = virt.addr() & ((1<<12) - 1);

        if self.pt_entries[pte_idx].is_empty() {
            return None;
        }

        let frame_addr = self.pt_entries[pte_idx].frame_addr()
            .expect("already checked").addr();

        let phys_addr = (frame_addr & !((1<<12) - 1)) | offset;

        Some(PhysAddr::new(phys_addr))
    }
}

