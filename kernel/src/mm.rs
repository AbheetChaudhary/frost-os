use crate::{address, page_tables};

use address::{PhysAddr, PhysRange};
use page_tables::PagingError;

pub trait FrameAllocator {
    fn allocate_frame(&mut self) -> Result<PhysAddr, PagingError>;
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LapicId(u8);

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

    /// Physical memory that backs kernel read-write. Each core has its own
    /// physical memory for that.
    KernelData(LapicId),

    /// Physical memory that backs kernel heap. Each core has its own physical
    /// memory for its heap.
    KernelHeap(LapicId),

    /// Physical memory that backs the shared heap. Common among all cores.
    KernelSharedHeap,

    BiosRegion,

    ReservedMemoryType,
}

/// A region of physical memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysMemRegion {
    /// Type of the physical memory region.
    mem_type: PhysMemType,

    /// Address range belonging to this region. This is guaranteed to be 4K
    /// aligned.
    addr_range: PhysRange,
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
    pub fn new_usable() -> FrameInfo {
        Self(0 | FRAME_INFO_USABLE)
    }

    /// Create new frame info for an unusable frame.
    #[inline]
    pub fn new_unusable() -> FrameInfo {
        Self(0)
    }

    /// Check whether the frame is usable or not.
    #[inline]
    pub fn is_usable(&self) -> bool {
        (self.0 & FRAME_INFO_USABLE) > 0
    }

    /// Check whether the frame is being used or not.
    ///
    /// # Panics
    ///
    /// This function panics if the check is being performed on a frame that is
    /// not even usable.
    pub fn is_being_used(&self) -> bool {
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
    pub fn set_used(&mut self) {
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
    pub fn clear_used(&mut self) {
        if !self.is_usable() {
            panic!("frame is not usable");
        }

        self.0 & !FRAME_INFO_USED;
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
