use crate::{address, mm, as_cast, cpu};

use address::{PhysAddr, VirtAddr};
use mm::{FrameAllocator};

/// Virtual address at which serialized pages from the page table data
/// structures will exist.
///
/// CR3 contains a physical address 
pub const PAGE_TABLE_START: VirtAddr = VirtAddr::new(32 * 0x100000);

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

    PdeFrameMissing,
}

#[repr(C, align(4096))]
pub struct PageDirectory {
    pd_entries: [PageDirEntry; 1024],
}

impl PageDirectory {
    unsafe fn insert_mapping<F: FrameAllocator>(
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

    pub unsafe fn insert_pd_entry(&mut self, pde_idx: usize,  phys: PhysAddr) 
        -> Option<()> {
        todo!();
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

            if let Some(pt_idx) = pt.contains(phys) {
                let pt_idx: usize = pt_idx.try_into().unwrap();
                let offset = phys.addr() & ((1<<12) - 1);
                let virt_addr = (pde_idx << 22) | (pt_idx << 12) | offset;
                return Some(VirtAddr::new(virt_addr));
            }
        }

        None
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
        let page_addr_u32: u32 = as_cast!(phys.addr(), usize => u32);

        // read/write, present.
        let pte = (page_addr_u32 & !((1<<12) - 1)) | (1<<1) | (1<<0);

        self.pt_entries[pte_idx] = PageTableEntry::new(pte);

        Ok(())
    }

    /// Get the page table entry index that contains the physical address of the
    /// `phys` frame address.
    pub fn contains(&self, phys: PhysAddr) -> Option<u32> {
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
}

