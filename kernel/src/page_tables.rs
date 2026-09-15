use crate::address::{PhysAddr, VirtAddr};

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
    let cr3 = crate::cpu::read_cr3();

    let pd_ptr: *const PageDirectory = core::ptr::with_exposed_provenance(cr3);

    let pd = unsafe { &*pd_ptr };

    let virt = pd.translate_phys_to_virt(phys);

    virt
}

#[repr(C, align(4096))]
pub struct PageDirectory {
    pd_entries: [PageDirEntry; 1024],
}

impl PageDirectory {
    pub fn translate_phys_to_virt(&self, phys: PhysAddr) -> Option<VirtAddr> {
        for pde_idx in 0..self.pd_entries.len() {
            if self.pd_entries[pde_idx].is_empty() {
                continue;
            }

            let pt_addr = self.pd_entries[pde_idx].pt_addr()
                .expect("already checked that the PDE mapping exists.");

            let pt = unsafe {
                let pt_ptr: *const PageTable =
                    core::ptr::with_exposed_provenance(pt_addr.addr());

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
