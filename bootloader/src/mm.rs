#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum PhysMemType {
    AcpiMemoryNvs,
    AcpiReclaimMemory,

    RuntimeServicesCode,
    RuntimeServicesData,

    /// Unused memory that could be used after allocating.
    UnusedMemory,

    BootServicesCode,
    BootServicesData,

    LoaderCode,
    LoaderData,
    LoaderStack,

    KernelCode,
    KernelStack,
    KernelData,
    KernelRodata,

    /// Any unused region below the first 1MiB in the physical address space.
    BiosRegion,

    ReservedMemoryType,
}

impl PhysMemType {
    /// Checks whether the memory type is `PhysMemType::UnusedMemory`.
    pub fn is_unused_memory(&self) -> bool {
        *self == PhysMemType::UnusedMemory
    }

    /// Checks whether memory of type `self` could be allocated or not.
    ///
    /// Other than `UnusedMemory` and `ReservedMemoryType`, all other
    /// types could be allocated.
    pub fn is_allocatable(&self) -> bool {
        use PhysMemType as PMT;

        *self != PMT::UnusedMemory && *self != PMT::ReservedMemoryType
    }
}

/// A trait for addresses.
///
/// `Self::from(self.addr()) == self` must be always true.
pub trait Address: Copy {
    /// Address represented by the `Self` type interpreted as a `usize`.
    fn addr(self) -> usize;

    /// Creating `Self` from an address of type `usize`.
    fn from(addr: usize) -> Self;

    fn checked_add(self, n: usize) -> Option<Self> {
        if let Some(sum) = self.addr().checked_add(n) {
            Some(Self::from(sum))
        } else {
            None
        }
    }

    fn checked_sub(self, n: usize) -> Option<Self> {
        if let Some(diff) = self.addr().checked_sub(n) {
            Some(Self::from(diff))
        } else {
            None
        }
    }

    fn checked_add_signed(self, n: isize) -> Option<Self> {
        if let Some(sum) = self.addr().checked_add_signed(n) {
            Some(Self::from(sum))
        } else {
            None
        }
    }

    fn checked_sub_signed(self, n: isize) -> Option<Self> {
        if let Some(diff) = self.addr().checked_sub_signed(n) {
            Some(Self::from(diff))
        } else {
            None
        }
    }

    fn strict_add(self, n: usize) -> Self {
        let sum = self.addr().strict_add(n);
        Self::from(sum)
    }

    fn strict_sub(self, n: usize) -> Self {
        let diff = self.addr().strict_sub(n);
        Self::from(diff)
    }

    fn strict_add_signed(self, n: isize) -> Self {
        let sum = self.addr().strict_add_signed(n);
        Self::from(sum)
    }

    fn strict_sub_signed(self, n: isize) -> Self {
        let diff = self.addr().strict_sub_signed(n);
        Self::from(diff)
    }

    #[inline]
    fn is_aligned<T>(self) -> bool {
        self.addr() % size_of::<T>() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct PhysAddr(usize);

impl PhysAddr {
    pub const fn new(addr: usize) -> PhysAddr {
        Self(addr)
    }

    pub fn as_virt_addr(self) -> VirtAddr {
        VirtAddr(self.addr())
    }

    /// Interpret `self` as a pointer to `T` and read it.
    ///
    /// # Safety
    ///
    /// - `self.addr()` should be a well aligned address of an initialized value
    ///   of type `T`.
    /// - Must be used with no paging or with identity mapped addresses.
    pub unsafe fn read<T>(self) -> T {
        let ptr: *const T = core::ptr::with_exposed_provenance(self.addr());

        unsafe { ptr.read() }
    }

    /// Interpret `self` as a pointer to `T` and read it. The address could be
    /// unaligned.
    ///
    /// # Safety
    ///
    /// - `self.addr()` should be an address of an initialized value of type
    ///   `T`.
    /// - Must be used with no paging or with identity mapped addresses.
    pub unsafe fn read_unaligned<T>(self) -> T {
        let ptr: *const T = core::ptr::with_exposed_provenance(self.addr());

        unsafe { ptr.read_unaligned() }
    }

    /// Interpret `self` as a pointer to `T` and write `val` to it without
    /// dropping the old value.
    ///
    /// # Safety
    ///
    /// - `self.addr()` should be a well aligned address for `T`.
    /// - Must be used with no paging or with identity mapped addresses.
    pub unsafe fn write<T>(self, val: T) {
        let ptr: *mut T = core::ptr::with_exposed_provenance_mut(self.addr());

        unsafe { ptr.write(val) }
    }

    /// Interpret `self` as a pointer to `T` and write `val` to it without
    /// dropping the old value. The address could be unaligned.
    ///
    /// # Safety
    ///
    /// - `self.addr()` should be a valid address for `T`.
    /// - Must be used with no paging or with identity mapped addresses.
    pub unsafe fn write_unaligned<T>(self, val: T) {
        let ptr: *mut T = core::ptr::with_exposed_provenance_mut(self.addr());

        unsafe { ptr.write_unaligned(val) }
    }

    pub unsafe fn as_ref<'a, T>(self) -> &'a T {
        let ptr: *const T = core::ptr::with_exposed_provenance(self.addr());

        unsafe { &*ptr }
    }

    pub unsafe fn as_mut<'a, T>(self) -> &'a mut T {
        let ptr: *mut T = core::ptr::with_exposed_provenance_mut(self.addr());

        unsafe { &mut *ptr }
    }
}

impl Address for PhysAddr {
    fn addr(self) -> usize {
        self.0
    }

    fn from(addr: usize) -> Self {
        Self::new(addr)
    }
}

/// A non-empty address range in the physical address space.
///
/// It could represent all possible address ranges, addressable by a `usize`,
/// except the range that contains all addresses from 0 to `usize::MAX`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PhysAddrRange {
    first: PhysAddr, // inclusive
    last: PhysAddr, // inclusive, greater than or equal to first.
}

impl PhysAddrRange {
    pub fn new(first: PhysAddr, last: PhysAddr) -> Result<PhysAddrRange, ()> {
        if first.addr() > last.addr() {
            return Err(());
        }

        // Problematic region whose size could not be represented in a usize.
        //
        // Creation of such a region is not allowed. Assuming degenerate program
        // behavior, panicking.
        if first.addr() == 0 && last.addr() == usize::MAX {
            panic!("Attempted to create a region whose size in bytes coule not \
                be represented in a usize.");
        }

        let r = Self {
            first,
            last,
        };

        Ok(r)
    }

    pub unsafe fn new_unchecked(first: PhysAddr, last: PhysAddr) -> PhysAddrRange {
        let r = Self {
            first,
            last,
        };

        r
    }

    pub fn first(&self) -> PhysAddr {
        self.first
    }

    pub fn last(&self) -> PhysAddr {
        self.last
    }

    /// Get the address of the first byte in the address range.
    pub fn first_addr(&self) -> usize {
        self.first.addr()
    }

    /// Get the address of the last byte in the address range.
    pub fn last_addr(&self) -> usize {
        self.last.addr()
    }

    /// Size, in bytes, of the address range.
    pub fn size(&self) -> usize {
        let first: usize = self.first_addr();
        let last: usize = self.last_addr();

        // This wouldn't panic as long as nothing ever creates a region that
        // encompasses the entire physical address space representable by a
        // usize. It is unlikely to happen in 64-bit, but might happen in
        // 32-bit. PhysAddrRange already panics upon trying to create such a
        // range.
        last.strict_sub(first).strict_add(1)
    }

    /// If `self` and `other` overlap, then get the overlapping address range.
    ///
    /// If no overlap exists then return `None`.
    pub fn overlap(&self, other: PhysAddrRange) -> Option<PhysAddrRange> {
        let first = self.first.max(other.first);
        let last = self.last.min(other.last);

        if first <= last {
            Some(unsafe {
                PhysAddrRange::new_unchecked(first, last)
            })
        } else {
            None
        }
    }

    /// Checks whether the `other` range is contained within `self`.
    pub fn contains_range(&self, other: PhysAddrRange) -> bool {
        if let Some(overlap) = self.overlap(other) {
            overlap == other
        } else {
            false
        }
    }

    /// Checks whether the range `self` contains the address `addr`.
    pub fn contains(&self, addr: PhysAddr) -> bool {
        self.first_addr() <= addr.addr() && addr.addr() <= self.last_addr()
    }

    /// Modifies `self` by taking the range `to_take` from the beginning of
    /// `self`. Returns the taken region.
    ///
    /// It returns `None` if both ranges do not begin at the same address or if
    /// both ranges are equal. If both ranges are equal then `self` would end up
    /// as an empty range which is not allowed.
    pub fn take_from_beginning(&mut self, to_take: PhysAddrRange)
        -> Option<PhysAddrRange>
    {
        // Both ranges must begin at the same address.
        if self.first != to_take.first {
            return None;
        }

        // to_take must end strictly before self.
        if to_take.last >= self.last {
            return None;
        }

        // Everything from one past the end of to_take to the end of
        // self(inclusive) remains with self, rest gets taken.
        self.first = to_take.last.checked_add(1)?;

        Some(to_take)
    }

    /// Checks whether `self` is neighbours with the `other` range. Two ranges
    /// being a neighbour means one begins just after the other one.
    pub fn is_neighbour_with(&self, other: PhysAddrRange) -> bool {
        // One past the last address of self should exist. If it exist then
        // check whether it is equal to the beginning of the other range.
        if let Some(one_past_self) = self.last.checked_add(1) {
            if one_past_self == other.first {
                return true;
            }
        }

        // Similarly check whether self exists one past the other range.
        if let Some(one_past_other) = other.last.checked_add(1) {
            if one_past_other == self.first {
                return true;
            }
        }

        false
    }

    /// Creates a new address range by merging `self` with `other`.
    ///
    /// Returns `None` if `self` and `other` are not neighbours.
    pub fn merge(&self, other: PhysAddrRange) -> Option<PhysAddrRange> {
        if !self.is_neighbour_with(other) {
            return None;
        }

        if self.first < other.first {
            PhysAddrRange::new(self.first, other.last).ok()
        } else {
            PhysAddrRange::new(other.first, self.last).ok()
        }
    }
}

/// A non-empty and contiguous region of physical memory.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PhysMemRegion {
    mem_type: PhysMemType,

    /// Address range associated with the memory region. It begins at a page
    /// boundary and the end is rounded up to the next available page boundary.
    addr_range: PhysAddrRange,
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
    pub fn new(addr_range: PhysAddrRange, mem_type: PhysMemType)
        -> Result<PhysMemRegion, ()>
    {
        if addr_range.first_addr() % (1<<12) != 0 {
            return Err(());
        }

        if addr_range.size() % (1<<12) != 0 {
            return Err(());
        }

        let r = Self {
            mem_type,
            addr_range,
        };

        Ok(r)
    }

    pub fn mem_type(&self) -> PhysMemType {
        self.mem_type
    }

    pub fn addr_range(&self) -> PhysAddrRange {
        self.addr_range
    }

    pub fn first(&self) -> PhysAddr {
        self.addr_range.first()
    }

    pub fn last(&self) -> PhysAddr {
        self.addr_range.last()
    }

    /// Get the address of the first byte in the address range.
    pub fn first_addr(&self) -> usize {
        self.addr_range.first_addr()
    }

    /// Get the address of the last byte in the address range.
    pub fn last_addr(&self) -> usize {
        self.addr_range.last_addr()
    }

    /// Checks whether `self` overlaps with the `other` physical memory region.
    pub fn overlaps_with(&self, other: PhysMemRegion) -> bool {
        self.addr_range.overlap(other.addr_range).is_some()
    }

    pub fn contains(&self, addr: PhysAddr) -> bool {
        self.addr_range.contains(addr)
    }

    pub fn size(&self) -> usize {
        self.addr_range.size()
    }

    pub fn page_count(&self) -> usize {
        let size = self.size();

        // We are guaranteed to not have any remainders.
        debug_assert!(size % (1<<12) == 0);

        size / (1<<12)
    }

    /// Checks whether this physical memory region is the all encompassing
    /// region which contains all physical addresses that are possible within a
    /// `usize`.
    pub fn is_universal_region(&self) -> bool {
        self.addr_range.first_addr() == 0 && 
            self.addr_range.last_addr() == usize::MAX
    }
}

/// Maximum number of memory regions possible with `PhysMem`.
///
/// For 64-bit architecture this comes around 170, which is not a lot UEFI
/// memory map. But it is more than enough if we ignore all the bootservices
/// code and data regions; which is a reasonable assumption becasuse `PhysMem`
/// comes in picture when we are ready to ditch boot services.
const MAX_REGION_COUNT: usize = (1 << 12) / size_of::<Option<PhysMemRegion>>();

#[repr(C, align(4096))]
pub struct PhysMem {
    regions: [Option<PhysMemRegion>; MAX_REGION_COUNT],
    active_region_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Error {
    RegionOverlap,
    AllSlotsOccupied,
    InvalidMemType,
    IntegerOverflow,
    AllocError,
    UnknownRegion,
    EmptyRegion,
    BigIntError,
    InvalidAddrRange,
    InvalidRegion,
}

impl PhysMem {
    /// Create a new empty `PhysMem` instance. Preferably there should not be
    /// more than one instance of `PhysMem` managing the physical memory.
    ///
    /// If there are more than one instance then only non-overlapping regions
    /// must be registered to them.
    pub fn new_empty() -> PhysMem {
        assert!(size_of::<PhysMem>() <= (1 << 12), 
            "PhysMem takes more than 4096 bytes.");

        Self {
            regions: [None; _],
            active_region_count: 0,
        }
    }

    /// Register a physical memory region.
    ///
    /// If the region overlaps with any of the existing regions or if there are
    /// no more slots to register another region then this function returns
    /// `Error`.
    ///
    /// # Safety
    ///
    /// - The region should be a valid memory region in the available physical
    ///   address space.
    ///
    /// - This region must not overlap with any region registered by any other
    ///   instance of `PhysMem`. As long as there is only one instance of
    ///   `PhysMem` this requirement will be trivially satisfied.
    pub unsafe fn register(&mut self, region: PhysMemRegion) -> Result<(), Error> {
        // A slot where we could put the next region, if it passes all the
        // checks. Initially this is set to a sentinel value, but in the end if
        // there are empty slots then it will be a valid index.
        let mut slot = MAX_REGION_COUNT;

        for (i, opt_region) in self.regions.iter().enumerate() {
            match opt_region {
                Some(r) => {
                    // If the region r overlaps with the to be registered region
                    // then fail with Err.
                    if r.overlaps_with(region) {
                        return Err(Error::RegionOverlap);
                    }
                }

                None => {
                    // Found an empty region slot that could be used to
                    // eventually register the new region.
                    //
                    // There might be multiple empty region slots like this, use
                    // the minimum one so that new regions fill the first empty
                    // slot.
                    slot = slot.min(i);
                }
            }
        }

        // If there are no empty slots to register the new region then return
        // err.
        if slot == MAX_REGION_COUNT {
            return Err(Error::AllSlotsOccupied);
        }

        self.regions[slot] = Some(region);

        self.active_region_count += 1;

        Ok(())
    }

    /// Gets the maximum number of non-overlapping physical memory regions that
    /// could be registered with `PhysMem`.
    pub fn max_region_slots(&self) -> usize {
        MAX_REGION_COUNT
    }

    /// Gets the number of non-overlapping regions currently existing in
    /// physical memory.
    ///
    /// This includes regions that were registered during initialization and any
    /// allocated/deallocated regions afterwards.
    pub fn active_region_count(&self) -> usize {
        self.active_region_count
    }

    /// Allocate a new non-empty region of memory of type `mem_type`.
    pub fn allocate_region(&mut self, page_count: usize, mem_type: PhysMemType)
        -> Result<PhysMemRegion, Error>
    {
        if page_count == 0 {
            return Err(Error::EmptyRegion);
        }

        if !mem_type.is_allocatable() {
            return Err(Error::InvalidMemType);
        }

        if self.active_region_count() == self.max_region_slots() {
            return Err(Error::AllSlotsOccupied);
        }

        // Find a big enough chunk of unused memory and carve a new region
        // from that.

        let big_chunk_mut = self.regions.iter_mut().find_map(|opt_region| {
            opt_region.as_mut().and_then(|r| {
                if r.mem_type == PhysMemType::UnusedMemory &&
                    r.page_count() >= page_count {
                        Some(r)
                } else {
                    None
                }
            })
        });

        // Index of the unused memory chunk from which we are going to
        // carve out a new allocation.
        let big_chunk_mut = big_chunk_mut.ok_or(Error::AllocError)?;

        // If big chunk is exactly equal to the number of pages we need, then
        // just modify it inplace, otherwise if it is bigger then carve out a
        // region from its beginning.

        if big_chunk_mut.page_count() == page_count {
            big_chunk_mut.mem_type = mem_type;

            // No need to increment active region count.

            Ok(*big_chunk_mut)
        } else {
            // Big chunk region is bigger than what we need.

            let new_region_first = big_chunk_mut.addr_range.first;

            // The subtraction won't panic, the multiplication is guaranteed to
            // be greater than zero.
            let new_region_last = new_region_first
                .checked_add(page_count.checked_mul(1 << 12)
                    .ok_or(Error::IntegerOverflow)?.strict_sub(1))
                .ok_or(Error::IntegerOverflow)?;

            let new_region_addr_range = unsafe {
                PhysAddrRange::new_unchecked(new_region_first, new_region_last)
            };

            // Modify big chunk in place to carve out a new region from the its
            // beginning.
            big_chunk_mut.addr_range.take_from_beginning(new_region_addr_range)
                .ok_or(Error::AllocError)?;

            let new_region = PhysMemRegion {
                mem_type: mem_type,

                addr_range: new_region_addr_range,
            };

            // This won't fail as we have already verified that there is atleast
            // one slot remaining.
            let empty_slot = self.regions.iter().position(|s| s.is_none())
                .ok_or(Error::AllSlotsOccupied)?;

            // Insert the newly created region in self.
            self.regions[empty_slot] = Some(new_region);

            // Increment the active region count.
            self.active_region_count += 1;

            Ok(new_region)
        }
    }

    pub unsafe fn deallocate_region(&mut self, region: PhysMemRegion)
        -> Result<(), Error>
    {
        if region.page_count() == 0 {
            return Err(Error::EmptyRegion);
        }

        // Only allocatable memory could be de-allocated.
        if !region.mem_type.is_allocatable() {
            return Err(Error::InvalidMemType);
        }

        // Get the index of this allocated region. If it is not part of the
        // physical memory then return err.
        let region_idx = self.regions.iter().position(|opt_region| {
            if let Some(r) = opt_region {
                *r == region
            } else {
                false
            }
        }).ok_or(Error::UnknownRegion)?;

        // If a unused memory neighbour exists for region, then merge.
        if let Some(neighbour_idx) = self.locate_unused_neighbour(region) {
            let neighbour_range = self.regions[neighbour_idx].as_mut()
                .expect("neighbour_idx contains a Some");

            // Merge the region with its neighbour.
            neighbour_range.addr_range = neighbour_range.addr_range
                .merge(region.addr_range)
                .expect("both ranges are guaranteed neighbours");

            // Overwrite region in self.
            self.regions[region_idx] = None;
        } else {
            // Declare region as unused memory, thus freeing it for future
            // use.
            self.regions[region_idx]
                .as_mut()
                .expect("region_idx contains a Some")
                .mem_type = PhysMemType::UnusedMemory;
        }

        self.active_region_count -= 1;

        Ok(())
    }

    /// Returns an iterator over all active regions.
    pub fn active_regions(&self) -> ActiveRegions<'_> {
        ActiveRegions {
            regions: self.regions.iter(),
        }
    }

    /// Returns an iterator over all active regions that allows modifying the
    /// regions.
    pub fn active_regions_mut(&mut self) -> ActiveRegionsMut<'_> {
        ActiveRegionsMut {
            regions: self.regions.iter_mut(),
        }
    }

    /// Locates a region that is neighbours with `region` and has unused
    /// memory type.
    fn locate_unused_neighbour(&self, region: PhysMemRegion) -> Option<usize> {
        self.regions.iter().position(|opt_region| {
            if let Some(r) = opt_region {
                r.addr_range.is_neighbour_with(region.addr_range) &&
                    r.mem_type.is_unused_memory()
            } else {
                false
            }
        })
    }
}

/// Immutable region iterator.
///
/// This struct is created by the method `active_regions` on `PhysMem`.
pub struct ActiveRegions<'a> {
    regions: core::slice::Iter<'a, Option<PhysMemRegion>>,
}

impl<'a> Iterator for ActiveRegions<'a> {
    type Item = &'a PhysMemRegion;

    fn next(&mut self) -> Option<Self::Item> {
        self.regions.find_map(Option::as_ref)
    }
}

/// Mutable region iterator.
///
/// This struct is created by the method `active_regions_mut` on `PhysMem`.
pub struct ActiveRegionsMut<'a> {
    regions: core::slice::IterMut<'a, Option<PhysMemRegion>>,
}

impl<'a> Iterator for ActiveRegionsMut<'a> {
    type Item = &'a mut PhysMemRegion;

    fn next(&mut self) -> Option<Self::Item> {
        self.regions.find_map(Option::as_mut)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VirtAddr(usize);

impl VirtAddr {
    pub const fn new(addr: usize) -> VirtAddr {
        Self(addr)
    }

    pub fn as_phys_addr(self) -> PhysAddr {
        PhysAddr::new(self.addr())
    }
}

impl Address for VirtAddr {
    fn addr(self) -> usize {
        self.0
    }

    fn from(addr: usize) -> Self {
        Self::new(addr)
    }
}

impl TryFrom<PhysAddr64> for PhysAddr {
    type Error = core::num::TryFromIntError;

    fn try_from(val: PhysAddr64) -> Result<Self, Self::Error> {
        val.as_u64().try_into().map(Self::new)
    }
}

impl TryFrom<PhysAddr32> for PhysAddr {
    type Error = core::num::TryFromIntError;

    fn try_from(val: PhysAddr32) -> Result<Self, Self::Error> {
        val.as_u32().try_into().map(Self::new)
    }
}

/// A 64-bit physical address.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr64(u64);

impl PhysAddr64 {
    /// Create a new 64-bit physical address.
    pub const fn new(addr: u64) -> PhysAddr64 {
        Self(addr)
    }

    /// Get the address as a `u64`.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// A 32-bit physical address.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PhysAddr32(u32);

impl PhysAddr32 {
    /// Create a new 32-bit physical address.
    pub const fn new(addr: u32) -> PhysAddr32 {
        Self(addr)
    }

    /// Get the address as a `u32`.
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

