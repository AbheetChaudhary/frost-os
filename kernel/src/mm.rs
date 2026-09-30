use core::{
    alloc::{
        Allocator,
        AllocError,
        Layout,
    },
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr::NonNull,
    sync::atomic::{
        AtomicPtr,
        Ordering
    },
};

use crate::{address, page_tables};

use address::{PhysAddr, PhysRange, VirtAddr};
use page_tables::PagingError;

pub trait FrameAllocator {
    fn allocate_frame(&mut self) -> Result<PhysAddr, PagingError>;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LapicId(pub u32);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PhysMemType {
    AcpiMemoryNvs,
    AcpiReclaimMemory,

    RuntimeServicesCode,
    RuntimeServicesData,

    UnusedMemory,

    /// Physical memory that backs kernel code pages. Common among all cores.
    KernelCode,

    /// Physical memory that backs kernel read-only data pages. Common among all
    /// cores.
    KernelRodata,

    /// Physical memory that backs kernel stack. Each core has its own physical
    /// memory for that.
    KernelStack(LapicId),

    /// Physical memory that backs kernel read-write data.
    ///
    /// The ELF file will have this as RW- segment, but in reality it is a read
    /// only segment. There will be no mutable globals, not even interior
    /// mutable globals in this kernel. Every exception will be eventually dealt
    /// with. TODO.
    KernelData,

    /// Physical memory that backs kernel heap. Each core has its own physical
    /// memory for its heap.
    KernelHeap(LapicId),

    /// Physical memory that backs the shared heap. Common among all cores.
    KernelSharedHeap,

    PagingData,

    BiosRegion,

    ReservedMemoryType,

    // Then there is the trampoline page sitting in the lower 1MB somewhere.
}

/// A region of physical memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysMemRegion {
    /// Type of the physical memory region.
    pub mem_type: PhysMemType,

    /// Address range belonging to this region. This is guaranteed to be 4K
    /// aligned.
    pub addr_range: PhysRange,
}

impl PhysMemRegion {
    /// Create a new physical memory region using an address range and memory
    /// type.
    ///
    /// The region's address range must begin at a page aligned address and must
    /// contain contain an integral multiple of 4KiB bytes. If that's not true
    /// then return `Err`.
    ///
    /// Creating a memory region is safe, but registering this region with
    /// `PhysMem` requires unsafe.
    pub fn new(addr_range: PhysRange, mem_type: PhysMemType)
        -> Result<PhysMemRegion, ()>
    {
        if addr_range.start_addr() % (1<<12) != 0 {
            return Err(());
        }

        if addr_range.len() % (1<<12) != 0 {
            return Err(());
        }

        let r = Self {
            mem_type,
            addr_range,
        };

        Ok(r)
    }

    pub fn page_count(&self) -> usize {
        let size = self.addr_range.len();

        // We are guaranteed to not have any remainders.
        debug_assert!(size % (1<<12) == 0);

        size / (1<<12)
    }

    pub unsafe fn set_mem_type(&mut self, mem_type: PhysMemType) {
        self.mem_type = mem_type;
    }

    pub fn mem_type(&self) -> PhysMemType {
        self.mem_type
    }

    pub fn addr_range(&self) -> PhysRange {
        self.addr_range
    }

    pub fn frame_iter(&self) -> FrameIterator {
        FrameIterator {
            next: self.start(),
            last: self.last(),
        }
    }
}

#[derive(Clone)]
pub struct FrameIterator {
    next: PhysAddr,
    last: PhysAddr,
}

impl Iterator for FrameIterator {
    type Item = PhysAddr;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next > self.last {
            return None;
        }

        let ret = self.next;

        self.next = self.next.strict_add(1<<12);

        Some(ret)
    }
}

impl core::ops::Deref for PhysMemRegion {
    type Target = PhysRange;

    fn deref(&self) -> &Self::Target {
        &self.addr_range
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct FrameInfo(u8);

/// Whether the frame is currently in use.
const FRAME_INFO_USED: u8 = 1 << 0;

/// Whether the frame could be used.
const FRAME_INFO_USABLE: u8 = 1 << 1;

impl FrameInfo {
    /// Create new frame info that could be later used.
    #[inline]
    pub const fn new_usable() -> FrameInfo {
        Self(0 | FRAME_INFO_USABLE)
    }

    /// Create new frame info for an unusable frame.
    #[inline]
    pub const fn new_unusable() -> FrameInfo {
        Self(0)
    }

    /// Check whether the frame is usable or not.
    #[inline]
    pub const fn is_usable(&self) -> bool {
        (self.0 & FRAME_INFO_USABLE) > 0
    }

    /// Check whether the frame is being used or not.
    ///
    /// # Panics
    ///
    /// This function panics if the check is being performed on a frame that is
    /// not even usable.
    pub const fn is_being_used(&self) -> bool {
        if !self.is_usable() {
            panic!("frame is not usable");
        }

        (self.0 & FRAME_INFO_USED) > 0
    }

    /// Mark the page as being used.
    ///
    /// # Panics
    ///
    /// This function panics if it is called on a frame that is not usable.
    pub const fn set_used(&mut self) {
        if !self.is_usable() {
            panic!("frame is not usable");
        }

        self.0 |= FRAME_INFO_USED;
    }

    /// Clear the 'in use' status of the frame.
    ///
    /// # Panics
    ///
    /// This function panics if it is called on a frame that is not even usable.
    pub const fn clear_used(&mut self) {
        if !self.is_usable() {
            panic!("frame is not usable");
        }

        self.0 &= !FRAME_INFO_USED;
    }
}

pub struct LightFrameAlloc<'a> {
    /// Status of all the frames that this light allocator has dominion over.
    frames: &'a mut [FrameInfo],

    /// The index of the first 4K frame.
    begin_frame_idx: usize,
}

impl<'a> FrameAllocator for LightFrameAlloc<'a> {
    fn allocate_frame(&mut self) -> Result<PhysAddr, PagingError> {
        // Index of a free frame.
        let free_idx = self.frames
            .iter()
            .position(|f| f.is_usable() && !f.is_being_used())
            .ok_or(PagingError::FrameAllocError)?;

        self.frames[free_idx].set_used();

        let frame_addr = (self.begin_frame_idx + free_idx).strict_mul(1<<12);

        Ok(PhysAddr::new(frame_addr))
    }
}

pub struct FrameAlloc {
    /// A pointer to frame info structs that store the current state of the
    /// frame.
    frames: *mut FrameInfo,

    /// The physical address of the first frame whose info is stored in
    /// `frames`.
    first_frame_addr: PhysAddr,

    /// The number of frame infos' pointed to by `frames`.
    count: usize,
}

