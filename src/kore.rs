#![allow(unused)]

//! Useful kernel abstractions.
//!
//! Normally in the userspace there is just one kind of pointer: the pointer to
//! some virtual memory. Languages don't need to be concerned with the fact that
//! the pointer does not points to an actual location in the RAM but to some
//! location in the virtual address space.
//!
//! But inside the kernel we must make that distinction and design all the
//! function API's with this distinction in mind.
//!
//! This module defines some pointer/pointer-like types for use specifically in
//! the kernel.
//!
//! The main pointer types: `VirtPtr<T>`, `VirtPtrMut<T>`, `PhysPtr<T>` and
//! `PhysPtrMut<T>` are just a wrapper on the Rust `*const T` and `*mut T`
//! pointer types. The only meaning they add is the distinction whether the
//! pointer points to virtual or physical address. They have similar pointer
//! operations defined on them as the Rust pointer types and have all the same
//! safety preconditions and some more.
//!
//! The extra safety preconditions are not fully decided yet but they will be
//! related to the virtual/physical nature of the location they point to. Expect
//! something along the lines:
//!
//! - While trying to read through a virtual pointer, it must have an allocated
//!   page table entry or it must be possible to make a page table entry for 
//!   that pointer if a page-fault occurs.
//!
//! - While trying to write through a `PhysPtrMut` the physical address space
//!   pointer must point to some memory region which is not in one of the 
//!   reserved memory regions and the kernel must be allowed to write to that 
//!   address, etc.

use core::fmt;

// Concerning Safety:
//
// - physical mapped pointer could be null.


use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::mem::MaybeUninit;

#[doc(hidden)]
pub use phys_ptr::{PhysAddr, PhysAddrRegion, PhysPtr, PhysPtrMut};

pub mod phys_ptr {
    //! Pointer types for physical addresses on the hardware.
    //!
    //! These are the kernel equivalent of the `*const T` and `*mut T` raw
    //! pointers with the added condition that they are meaningful only in the
    //! hardware physical address space.
    //!
    //! These pointers must not be mixed with virtual address space pointers or
    //! userspace virtual pointers.
    //!
    //! They have similar operations as their respective raw pointer
    //! counterparts with all their safety requirements and some more. Exactly
    //! what extra safety requirements they have is not completely decided yet.

    use super::*;

    /// An address in the physical address space with no type attached to it.
    ///
    /// This is like the underlying `usize` address of raw pointers, with the
    /// added meaning that this address is in the physical address space.
    /// Priamarily useful for doing arithmetics on physical addresses.
    ///
    /// All arithmetic operations on `PhysAddr` guarantee non-wrapping
    /// behaviour. Operations that cannot be represented either return
    /// `None` (`checked_*`) or panic (`strict_*`).
    ///
    /// A `PhysAddr` does not necessarily mean that it points to a valid
    /// physical address. It is just an abstraction over a `usize` with some
    /// methods that are helpful in arithmetic over addresses. When actually
    /// trying to read the memory from the physical address one does need to
    /// first convert it to `PhysPtr<T>` and follow its safety requirements over
    /// reading and writing from physical address space.
    #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[repr(transparent)]
    pub struct PhysAddr(#[doc(hidden)] usize);

    impl fmt::Debug for PhysAddr {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "PhysAddr({:p})", self.addr() as *const ())
        }
    }

    impl core::iter::Step for PhysAddr {
        fn steps_between(start: &Self, end: &Self) -> (usize, Option<usize>) {
            let start: usize = start.addr();
            let end: usize = end.addr();

            <usize as core::iter::Step>::steps_between(&start, &end)
        }

        fn forward_checked(start: Self, count: usize) -> Option<Self> {
            start.checked_add(count)
        }

        fn forward_overflowing(start: Self, count: usize) -> (Self, bool) {
            let start: usize = start.addr();

            let (addr, overflow) = <usize as core::iter::Step>::
                forward_overflowing(start, count);

            (PhysAddr::new(addr), overflow)
        }

        fn backward_checked(start: Self, count: usize) -> Option<Self> {
            start.checked_sub(count)
        }

        fn backward_overflowing(start: Self, count: usize) -> (Self, bool) {
            let start: usize = start.addr();

            let (addr, overflow) = <usize as core::iter::Step>::
                backward_overflowing(start, count);

            (PhysAddr::new(addr), overflow)
        }

    }

    impl PhysAddr {
        /// Create a new physical address from a raw `usize`.
        pub const fn new(addr: usize) -> PhysAddr {
            Self(addr)
        }

        /// Makes an inclusive range starting at `self` and ending at `end`.
        pub const fn make_region_with_last(self, last: PhysAddr) -> PhysAddrRegion {
            let r = core::range::RangeInclusive {
                start: self,
                last:  last,
            };

            PhysAddrRegion::from_inclusive_range(r)
        }

        /// Makes an inclusive range containing `len` addresses starting at
        /// `self`.
        ///
        /// Returns `None` if the address range ends up wrapping or if `len` is
        /// zero.
        pub const fn make_region_with_len(self, len: usize) 
            -> Option<PhysAddrRegion> {

            if len == 0 {
                return None;
            }

            // Calculate the last address in this range.
            let last = match self.checked_add(len - 1) {
                Some(e) => e,
                None => return None,
            };

            let r = core::range::RangeInclusive {
                start: self,
                last:  last,
            };

            Some(PhysAddrRegion::from_inclusive_range(r))
        }

        /// Get the raw address as `usize`.
        pub const fn addr(self) -> usize {
            self.0
        }

        /// Strict addition of an integer with physical address.
        ///
        /// # Panics
        ///
        /// The function panics if the addition overflows, regardless of whether
        /// overflow checks are enabled.
        pub const fn strict_add(self, count: usize) -> PhysAddr {
            match self.addr().checked_add(count) {
                Some(addr) => Self(addr),
                None => panic!("strict_add: integer overflow occured"),
            }
        }

        /// Checked addition of an integer with physical address. Computes the
        /// new physical address or returns `None` if overflow occurs.
        pub const fn checked_add(self, count: usize) -> Option<PhysAddr> {
            if let Some(addr) = self.addr().checked_add(count) {
                Some(Self(addr))
            } else {
                None
            }
        }

        /// Strict addition of a signed integer with physical address.
        ///
        /// # Panics
        ///
        /// The function panics if the addition overflows, regardless of whether
        /// overflow checks are enabled.
        pub const fn strict_add_signed(self, count: isize) -> PhysAddr {
            match self.addr().checked_add_signed(count) {
                Some(addr) => Self(addr),
                None => panic!("strict_add_signed: integer overflow occured"),
            }
        }

        /// Checked addition of a signed integer with physical address. Computes
        /// the new physical address or returns `None` if overflow occurs.
        pub const fn checked_add_signed(self, count: isize) -> Option<PhysAddr> {
            if let Some(addr) = self.addr().checked_add_signed(count) {
                Some(Self(addr))
            } else {
                None
            }
        }

        /// Strict subtraction of an integer from a physical address.
        ///
        /// # Panics
        ///
        /// The function panics if the subtraction overflows, regardless of
        /// whether overflow checks are enabled.
        pub const fn strict_sub(self, count: usize) -> PhysAddr {
            match self.addr().checked_sub(count) {
                Some(addr) => Self(addr),
                None => panic!("strict_sub: integer overflow occured"),
            }
        }

        /// Checked subtraction of an integer from a physical address. Computes
        /// the new physical address or returns `None` if overflow occurs.
        pub const fn checked_sub(self, count: usize) -> Option<PhysAddr> {
            if let Some(addr) = self.addr().checked_sub(count) {
                Some(Self(addr))
            } else {
                None
            }
        }

        /// Strict subtraction of a signed integer from a physical address.
        ///
        /// # Panics
        ///
        /// The function panics if the subtraction overflows, regardless of
        /// whether overflow checks are enabled.
        pub const fn strict_sub_signed(self, count: isize) -> PhysAddr {
            match self.addr().checked_sub_signed(count) {
                Some(addr) => Self(addr),
                None => panic!("strict_sub_signed: integer overflow occured"),
            }
        }

        /// Checked subtraction of a signed integer from a physical address.
        /// Computes the new physical address or returns `None` if overflow
        /// occurs.
        pub const fn checked_sub_signed(self, count: isize) -> Option<PhysAddr> {
            if let Some(addr) = self.addr().checked_sub_signed(count) {
                Some(Self(addr))
            } else {
                None
            }
        }

        /// Checks whether the address is aligned to `align`. Returns `true` if
        /// it is aligned otherwise return `false`.
        ///
        /// # Panics
        ///
        /// The function panics if `align` is not a power-of-two.
        pub const fn is_aligned_to(self, align: usize) -> bool {
            if !align.is_power_of_two() {
                panic!("is_aligned_to: align is not a power-of-two");
            }

            (self.addr() & (align - 1)) == 0
        }

        /// Generate a new physical address less than or equal to `self` that is
        /// aligned to `align`.
        ///
        /// # Panics
        ///
        /// The function panics if `align` is not a power-of-two.
        pub const fn align_down(self, align: usize) -> PhysAddr {
            if !align.is_power_of_two() {
                panic!("align_down: align is not a power-of-two");
            }

            let addr = self.addr() & !(align - 1);

            Self(addr)
        }

        /// Generates a new physical address greater than or equal to `self`
        /// that is aligned to `align`. If the alignment could not be achieved
        /// without overflow then returns `None`.
        ///
        /// # Panics
        ///
        /// The function panics if `align` is not a power-of-two.
        pub const fn checked_align_up(self, align: usize) -> Option<PhysAddr> {
            if !align.is_power_of_two() {
                panic!("checked_align_up: align is not a power-of-two");
            }

            // Remainder upon dividing addr with align.
            let rem = self.addr() & (align - 1);

            // If address is already aligned, do nothing.
            if rem == 0 {
                return Some(self);
            }

            // Amount that should be added to addr to make it a multiple of
            // align.
            let to_add = align - rem;

            match self.addr().checked_add(to_add) {
                Some(aligned_up_addr) => Some(Self(aligned_up_addr)),
                None => None,
            }
        }

        /// Volatile read of a `u8` at physical address `self`.
        ///
        /// # Safety
        ///
        /// - The read must produce a properly initialized `u8`.
        ///
        /// - The physical address must be valid for reads or it must point to
        ///   memory outside of all Rust allocations and reading the memory
        ///   must:
        ///     - not trap, and
        ///     - not cause any memory inside any Rust allocation to be
        ///     modified.
        pub unsafe fn read_u8(self) -> u8 {
            // Pointers created without any provenance are not sound for any
            // read or writes, except for volatile accesses to addresses outside
            // the Rust memory model.

            let ptr = core::ptr::without_provenance::<u8>(self.addr());
            unsafe { ptr.read_volatile() }
        }

        /// Volatile read of a `u16` at physical address `self`.
        ///
        /// # Safety
        ///
        /// - The read must produce a properly initialized `u16`.
        ///
        /// - The physical address must be valid for reads or it must point to
        ///   memory outside of all Rust allocations and reading the memory
        ///   must:
        ///     - not trap, and
        ///     - not cause any memory inside any Rust allocation to be
        ///     modified.
        ///
        /// - The address must be properly aligned.
        pub unsafe fn read_u16(self) -> u16 {
            // For properly aligned addresses, they will always have enough
            // bytes to make a u16, regardless of how close they are to the
            // end of the physical address space.

            let ptr = core::ptr::without_provenance::<u16>(self.addr());
            unsafe { ptr.read_volatile() }
        }

        /// Volatile read of a `u16` at physical address `self`. The address
        /// does not need to be properly aligned.
        ///
        /// Returns `None` if `self` is too close to the end of the physical
        /// address space such that there aren't enough bytes to make a `u16`.
        ///
        /// # Safety
        ///
        /// - The read must produce a properly initialized `u16`.
        ///
        /// - The physical address must be valid for reads or it must point to
        ///   memory outside of all Rust allocations and reading the memory
        ///   must:
        ///     - not trap, and
        ///     - not cause any memory inside any Rust allocation to be
        ///     modified.
        pub unsafe fn read_u16_unaligned_checked(self) -> Option<u16> {
            let mut bytes = [MaybeUninit::new(0u8); 2];

            unsafe {
                self.read_into_buffer_checked(&mut bytes)?;
            }

            let bytes_initialized: [u8; 2] = unsafe {
                bytes.map(|b| unsafe { b.assume_init() })
            };

            Some(u16::from_ne_bytes(bytes_initialized))
        }

        /// Volatile read of a `u32` at physical address `self`.
        ///
        /// # Safety
        ///
        /// - The read must produce a properly initialized `u32`.
        ///
        /// - The physical address must be valid for reads or it must point to
        ///   memory outside of all Rust allocations and reading the memory
        ///   must:
        ///     - not trap, and
        ///     - not cause any memory inside any Rust allocation to be
        ///     modified.
        ///
        /// - The address must be properly aligned.
        pub unsafe fn read_u32(self) -> u32 {
            let ptr = core::ptr::without_provenance::<u32>(self.addr());
            unsafe { ptr.read_volatile() }
        }

        /// Volatile read of a `u32` at physical address `self`. The address
        /// does not need to be properly aligned.
        ///
        /// Returns `None` if `self` is too close to the end of the physical
        /// address space such that there aren't enough bytes to make a `u32`.
        ///
        /// # Safety
        ///
        /// - The read must produce a properly initialized `u32`.
        ///
        /// - The physical address must be valid for reads or it must point to
        ///   memory outside of all Rust allocations and reading the memory
        ///   must:
        ///     - not trap, and
        ///     - not cause any memory inside any Rust allocation to be
        ///     modified.
        pub unsafe fn read_u32_unaligned(self) -> Option<u32> {
            let mut bytes = [MaybeUninit::new(0u8); 4];

            unsafe {
                self.read_into_buffer_checked(&mut bytes)?
            }

            let bytes_initialized: [u8; 4] = unsafe {
                bytes.map(|b| unsafe { b.assume_init() })
            };

            Some(u32::from_ne_bytes(bytes_initialized))
        }

        /// Volatile read of a `u64` at physical address `self`.
        ///
        /// # Safety
        ///
        /// - The read must produce a properly initialized `u64`.
        ///
        /// - The physical address must be valid for reads or it must point to
        ///   memory outside of all Rust allocations and reading the memory
        ///   must:
        ///     - not trap, and
        ///     - not cause any memory inside any Rust allocation to be
        ///     modified.
        ///
        /// - The address must be properly aligned.
        pub unsafe fn read_u64(self) -> u64 {
            let ptr = core::ptr::without_provenance::<u64>(self.addr());
            unsafe { ptr.read_volatile() }
        }

        /// Volatile read of a `u64` at physical address `self`.
        ///
        /// Returns `None` if `self` is too close to the end of the physical
        /// address space such that there aren't enough bytes to make a `u64`.
        ///
        /// # Safety
        ///
        /// - The read must produce a properly initialized `u64`.
        ///
        /// - The physical address must be valid for reads or it must point to
        ///   memory outside of all Rust allocations and reading the memory
        ///   must:
        ///     - not trap, and
        ///     - not cause any memory inside any Rust allocation to be
        ///     modified.
        pub unsafe fn read_u64_unaligned(self) -> Option<u64> {
            let mut bytes = [MaybeUninit::new(0u8); 8];

            unsafe {
                self.read_into_buffer_checked(&mut bytes)?;
            }

            let bytes_initialized = unsafe {
                bytes.map(|b| unsafe { b.assume_init() })
            };

            Some(u64::from_ne_bytes(bytes_initialized))
        }

        /// Fill `buffer` by reading bytes from physical address `self`.
        ///
        /// # Panics
        ///
        /// The function panics if `self` is too close to the end of physical
        /// address space such that trying to read `buffer.len()` number of
        /// bytes causes wrapping around the physical address space.
        ///
        /// # Safety
        ///
        /// - `self` must be safe for volatile reads.
        pub unsafe fn read_into_buffer_strict(self, buffer: &mut [MaybeUninit<u8>]) {
            let len = buffer.len();
            let mut addr = self.addr();

            // Number of bytes read till now.
            let mut i = 0;

            // Read individual bytes until we get an alignment of 4.
            while i < len && (addr.strict_add(i) % 4) != 0 {
                let b = unsafe { self.strict_add(i).read_u8() };
                buffer[i] = MaybeUninit::new(b);

                i += 1;
            }

            // Read as many chunks of 4 as we can.
            // 
            // Index of the next byte that we are going to read, i.e. i and then
            // 3 bytes after that must be less than len.
            while i + 3 < len {
                let chunk = unsafe { self.strict_add(i).read_u32() };
                let bytes = chunk.to_ne_bytes();

                for j in 0..4 {
                    buffer[i + j].write(bytes[j]);
                }

                i += 4;
            }

            // Read remaining bytes.
            while i < len {
                let b = unsafe { self.strict_add(i).read_u8() };
                buffer[i] = MaybeUninit::new(b);

                i += 1;
            }
        }

        /// Fill `buffer` byte reading bytes from physical address `self`.
        ///
        /// Returns `None` if `self` was too close to the end of the physical
        /// address space, such that trying to read `buffer.len()` number of
        /// bytes would have caused an integer overflow.
        ///
        /// # Safety
        ///
        /// - `self` must be safe for volatile reads.
        pub unsafe fn read_into_buffer_checked(self, buffer: &mut [MaybeUninit<u8>])
            -> Option<()> {

            let len = buffer.len();
            let mut addr = self.addr();

            // Number of bytes read till now.
            let mut i = 0;

            // Read individual bytes until we get an alignment of 4.
            while i < len && (addr.checked_add(i)? % 4) != 0 {
                let b = unsafe { self.checked_add(i)?.read_u8() };
                buffer[i] = MaybeUninit::new(b);

                i += 1;
            }

            // Read as many chunks of 4 as we can.
            // 
            // Index of the next byte that we are going to read, i.e. i and then
            // 3 bytes after that must be less than len.
            while i + 3 < len {
                let chunk = unsafe { self.checked_add(i)?.read_u32() };
                let bytes = chunk.to_ne_bytes();

                for j in 0..4 {
                    buffer[i + j].write(bytes[j]);
                }

                i += 4;
            }

            // Read remaining bytes.
            while i < len {
                let b = unsafe { self.checked_add(i)?.read_u8() };
                buffer[i] = MaybeUninit::new(b);

                i += 1;
            }

            Some(())
        }
    }

    #[derive(Clone, Copy)]
    pub struct PhysAddrRegion(#[doc(hidden)] core::range::RangeInclusive<PhysAddr>);

    impl fmt::Debug for PhysAddrRegion {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "PhysAddrRegion(start={:p}, last={:p})",
                self.start.addr() as *const (),
                self.last.addr() as *const ())
        }
    }

    impl PhysAddrRegion {
        /// Returns the number of addresses in the address region. 
        ///
        /// If it contains all possible addresses then return `None`. That's
        /// because the number of unique values from `0` to `usize::MAX` could
        /// not be represented in `usize`.
        pub fn len(&self) -> Option<usize> {
            let len_off_by_one = self.0.last.addr().checked_sub(self.0.start.addr())?;

            len_off_by_one.checked_add(1)
        }

        pub const fn from_inclusive_range(range: core::range::RangeInclusive<PhysAddr>) 
            -> PhysAddrRegion {

            Self(range)
        }

        pub fn from_start_and_last(start: PhysAddr, last: PhysAddr) -> PhysAddrRegion {
            let range = core::range::RangeInclusive {
                start,
                last,
            };

            Self(range)
        }
    }

    impl core::ops::Deref for PhysAddrRegion {
        type Target = core::range::RangeInclusive<PhysAddr>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    /// `*const T` equivalent for the physical address space.
    ///
    /// This pointer can easily lie outside the Rust abstract machine. For
    /// example, it could be some firmware provided address. Rust requires that
    /// pointer arithmetic does not escapes the original allocation into which
    /// the pointer belongs.
    ///
    /// I don't know whether firmware provided addresses are part of what Rust
    /// calls an "allocation". That's why it is not entirely safe to perform
    /// pointer arithemetic on this type's underlying pointer using `add()` or
    /// `sub()` functions. Instead one must use `wrapping_add()` or
    /// `wrapping_sub()` functions as they delay the requirement of
    /// provenance/allocation for the time when one eventually tries to do a
    /// read/write through the shifted pointer. Now those reads/writes must be
    /// done with `read_volatile()` or `write_volatile()` only because these
    /// functions are special and completely ignore pointer provenance.
    ///
    /// So the best way to work with physical pointers is to create them without
    /// any provenance attached to them, use wrapping pointer arithmetic to
    /// avoid reasoning about provenance, and finally use volatile accesses that
    /// completely ignore provenance, whatever it may be.
    #[repr(transparent)]
    pub struct PhysPtr<T: ?Sized>(#[doc(hidden)] *const T);

    impl<T> Copy for PhysPtr<T> {}

    impl<T> Clone for PhysPtr<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    /// `*mut T` equivalent for the physical address space.
    ///
    /// Read more at [`PhysPtr<T>`](PhysPtr<T>).
    #[repr(transparent)]
    pub struct PhysPtrMut<T: ?Sized>(#[doc(hidden)] *mut T);

    impl<T> Copy for PhysPtrMut<T> {}

    impl<T> Clone for PhysPtrMut<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<T> PhysPtr<T> {
        /// Create a `PhysPtr<T>` from the given physical address with no
        /// provenance attached to it.
        ///
        /// Right now this is the only way to create a `PhysPtr<T>` from thin
        /// air. So all physical pointers are without any provenance. This might
        /// change in future.
        pub const fn without_provenance(addr: PhysAddr) -> PhysPtr<T> {
            let ptr = core::ptr::without_provenance::<T>(addr.addr());

            Self(ptr)
        }

        /// Creates a new `PhysPtr<T>` with given physical address and the
        /// provenance of `self`.
        pub fn with_phys_addr(self, addr: PhysAddr) -> PhysPtr<T> {
            let ptr = self.as_ptr().with_addr(addr.addr());

            Self(ptr)
        }

        /// Creates a new `PhysPtr<T>` by mapping `self`'s physical address to a
        /// new one, preserving the provenance of `self`.
        pub fn map_phys_addr<F>(self, f: F) -> PhysPtr<T>
            where F: FnOnce(PhysAddr) -> PhysAddr
        {
            let new_addr = f(self.phys_addr());

            self.with_phys_addr(new_addr)
        }

        /*
        /// Exposes the provenance part of the pointer and returns the physical
        /// address.
        ///
        /// Currently `PhysPtr<T>` could only be created without any provenance
        /// attached to it. So this function basically useless because there is
        /// no provenance to expose, but this might change in future.
        pub fn expose_provenance(self) -> PhysAddr {
            let addr = self.as_ptr().expose_provenance();

            PhysAddr(addr)
        }
        */

        /// Gets the physical address of the pointer without exposing the
        /// provenance.
        pub fn phys_addr(self) -> PhysAddr {
            PhysAddr(self.as_ptr().addr())
        }

        /// Gets the raw pointer underlying the `PhysPtr<T>` type.
        pub const fn as_ptr(self) -> *const T {
            self.0
        }

        pub const fn cast<U>(self) -> PhysPtr<U> {
            PhysPtr(self.as_ptr().cast::<U>())
        }

        pub const unsafe fn cast_mut(self) -> PhysPtrMut<T> {
            PhysPtrMut(self.as_ptr().cast_mut())
        }

        /// Adds an unsigned offset to a pointer using strict arithmetic.
        /// `count` is in units of `T`. The returned `PhysPtr<T>` has the same
        /// provenance as `self`.
        ///
        /// # Panics
        ///
        /// The function panics if the addition overflows, regardless of whether
        /// overflow checks are enabled.
        pub fn strict_add(self, count: usize) -> PhysPtr<T> {
            // Calculate the byte offset.
            let byte_offset = match count.checked_mul(size_of::<T>()) {
                Some(o) => o,
                None => panic!("strict_add: size_of::<T> * count overflows"),
            };

            // Calculate the checked physical address after addition.
            let addr = match self.phys_addr().checked_add(byte_offset) {
                Some(a) => a,
                None => panic!("strict_add: addition overflowed"),
            };

            self.with_phys_addr(addr)
        }

        /// Add an unsigned offset to pointer using strict arithmetic. `count`
        /// is in units of bytes. The returned `PhysPtr<T>` has the same
        /// provenance as `self`.
        ///
        /// # Panics
        ///
        /// The function panics if the addition overflows, regardless of whether
        /// overflow checks are enabled.
        pub fn strict_byte_add(self, count: usize) -> PhysPtr<T> {
            // Calculate the checked physical address after addition.
            let addr = match self.phys_addr().checked_add(count) {
                Some(a) => a,
                None => panic!("strict_byte_add: addition overflow"),
            };

            // Create a physical pointer with addr and provenance of self.
            self.with_phys_addr(addr)
        }

        /// Subtracts an unsigned offset from the pointer using strict
        /// arithemetic. `count` is in units of `T`. The returned `PhysPtr<T>`
        /// has the same provenance as `self`.
        ///
        /// # Panics
        ///
        /// The function panics if the subtraction overflows, regardless of
        /// whether overflow checks are enabled.
        pub fn strict_sub(self, count: usize) -> PhysPtr<T> {
            // Calculate the byte offset.
            let byte_offset = match count.checked_mul(size_of::<T>()) {
                Some(o) => o,
                None => panic!("strict_sub: size_of::<T> * count overflows"),
            };

            // Calculate the checked physical address after subtraction.
            let addr = match self.phys_addr().checked_sub(byte_offset) {
                Some(a) => a,
                None => panic!("strict_sub: subtraction overflowed"),
            };

            self.with_phys_addr(addr)
        }

        /// Subtract an unsigned offset to pointer using strict arithmetic.
        /// `count` is in units of bytes. The returned `PhysPtr<T>` has the same
        /// provenance as `self`.
        ///
        /// # Panics
        ///
        /// The function panics if the subtraction overflows, regardless of
        /// whether overflow checks are enabled.
        pub fn strict_byte_sub(self, count: usize) -> PhysPtr<T> {
            // Calculate the checked physical address after subtraction.
            let addr = match self.phys_addr().checked_sub(count) {
                Some(a) => a,
                None => panic!("strict_byte_sub: subtraction overflow"),
            };

            // Create a physical pointer with addr and provenance of self.
            self.with_phys_addr(addr)
        }

        /// Checked addition of an unsigned offset to a pointer. `count` is in
        /// units of `T`. The returned `PhysPtr<T>` has the same provenance as
        /// `self`.
        ///
        /// Returns `None` if `count` after being scaled with `size_of::<T>` is
        /// too bit to fit in `usize` or if the addition of offset overflows
        /// the pointer address.
        ///
        /// The pointer returned by this is guaranteed to not have gone through
        /// wrapping behaviour.
        pub fn checked_add(self, count: usize) -> Option<PhysPtr<T>> {
            // Calculate the resulting address after addition of scaled count.
            let addr = self.phys_addr()
                .checked_add(count.checked_mul(size_of::<T>())?)?;

            Some(self.with_phys_addr(addr))
        }

        /// Checked addition of an unsigned offset to a pointer. `count` is in
        /// units of bytes. The returned `PhysPtr<T>` has the same provenance as
        /// `self`.
        ///
        /// Returns `None` if the addition of offset overflows the pointer
        /// address.
        ///
        /// The pointer returned by this is guaranteed to not have gone through
        /// wrapping behaviour.
        pub fn checked_byte_add(self, count: usize) -> Option<PhysPtr<T>> {
            let addr = self.phys_addr().checked_add(count)?;

            Some(self.with_phys_addr(addr))
        }

        /// Checked subtraction of an unsigned offset from a pointer. `count` is
        /// in units of `T`. The returned `PhysPtr<T>` has the same provenance
        /// as `self`.
        ///
        /// Returns `None` if `count` after being scaled with `size_of::<T>` is
        /// too bit to fit in `usize` or if the subtraction of offset overflows
        /// the pointer address.
        ///
        /// The pointer returned by this is guaranteed to not have gone through
        /// wrapping behaviour.
        pub fn checked_sub(self, count: usize) -> Option<PhysPtr<T>> {
            // Calculate the resulting address after addition of scaled count.
            let addr = self.phys_addr()
                .checked_sub(count.checked_mul(size_of::<T>())?)?;

            Some(self.with_phys_addr(addr))
        }

        /// Checked subtraction of an unsigned offset to a pointer. `count` is
        /// in units of bytes. The returned `PhysPtr<T>` has the same provenance
        /// as `self`.
        ///
        /// Returns `None` if the subtraction of offset overflows the
        /// pointer address.
        ///
        /// The pointer returned by this is guaranteed to not have gone through
        /// wrapping behaviour.
        pub fn checked_byte_sub(self, count: usize) -> Option<PhysPtr<T>> {
            let addr = self.phys_addr().checked_sub(count)?;

            Some(self.with_phys_addr(addr))
        }

        /// Performs a volatile read the the physical address pointed to by
        /// `PhysPtr<T>`. This is the only right way to read memory pointed to
        /// by a physical pointer.
        ///
        /// # Safety
        ///
        /// - `self` is properly aligned for `T`.
        /// - The physical address is valid for a volatile read on the target
        ///   hardware.
        /// - The read does not trap.
        /// - The read produces a properly initialized value of type `T`.
        /// - The access does not violates any hardware specific requirements.
        /// - The accessed memory is not Rust-allocated memory unless the usual
        ///   Rust rules for volatile access to Rust allocations are satisfied.
        ///
        /// Also read the safety preconditions of `core::ptr::read_volatile`.
        pub unsafe fn read_volatile(self) -> T {
            unsafe {
                self.as_ptr().read_volatile()
            }
        }
    }

    impl<T> PhysPtrMut<T> {
        /// Create a `PhysPtrMut<T>` from the given physical address with no
        /// provenance attached to it.
        ///
        /// Right now this is the only way to create a `PhysPtrMut<T>` from thin
        /// air. So all physical pointers are without any provenance. This might
        /// change in future.
        pub const fn without_provenance(addr: PhysAddr) -> PhysPtrMut<T> {
            let ptr = core::ptr::without_provenance_mut::<T>(addr.addr());

            Self(ptr)
        }

        /// Creates a new `PhysPtrMut<T>` with given physical address and the
        /// provenance of `self`.
        pub fn with_phys_addr(self, addr: PhysAddr) -> PhysPtrMut<T> {
            let ptr = self.as_mut_ptr().with_addr(addr.addr());

            Self(ptr)
        }

        /// Creates a new `PhysPtrMut<T>` by mapping `self`'s physical address
        /// to a new one, preserving the provenance of `self`.
        pub fn map_phys_addr<F>(self, f: F) -> PhysPtrMut<T>
            where F: FnOnce(PhysAddr) -> PhysAddr
        {
            let new_addr = f(self.phys_addr());

            self.with_phys_addr(new_addr)
        }

        /*
        /// Exposes the provenance part of the pointer and returns the physical
        /// address.
        ///
        /// Currently `PhysPtrMut<T>` could only be created without any
        /// provenance attached to it. So this function basically useless
        /// because there is no provenance to expose, but this might change in
        /// future.
        pub fn expose_provenance(self) -> PhysAddr {
            let addr = self.as_mut_ptr().expose_provenance();

            PhysAddr(addr)
        }
        */

        /// Gets the physical address of the pointer without exposing the
        /// provenance.
        pub fn phys_addr(self) -> PhysAddr {
            PhysAddr(self.as_mut_ptr().addr())
        }

        /// Gets `PhysPtrMut<T>` as a `*const T` pointer.
        pub const fn as_ptr(self) -> *const T {
            self.0.cast_const()
        }

        /// Gets `PhysPtrMut<T>` as a `*mut T` pointer.
        pub const fn as_mut_ptr(self) -> *mut T {
            self.0
        }

        pub const fn cast<U>(self) -> PhysPtrMut<U> {
            PhysPtrMut(self.as_mut_ptr().cast::<U>())
        }

        pub const fn cast_const(self) -> PhysPtr<T> {
            PhysPtr(self.as_ptr())
        }

        /// Adds an unsigned offset to a pointer using strict arithmetic.
        /// `count` is in units of `T`. The returned `PhysPtrMut<T>` has the
        /// same provenance as `self`.
        ///
        /// # Panics
        ///
        /// The function panics if the addition overflows, regardless of whether
        /// overflow checks are enabled.
        pub fn strict_add(self, count: usize) -> PhysPtrMut<T> {
            // Calculate the byte offset.
            let byte_offset = match count.checked_mul(size_of::<T>()) {
                Some(o) => o,
                None => panic!("strict_add: size_of::<T> * count overflows"),
            };

            // Calculate the checked physical address after addition.
            let addr = match self.phys_addr().checked_add(byte_offset) {
                Some(a) => a,
                None => panic!("strict_add: addition overflowed"),
            };

            self.with_phys_addr(addr)
        }

        /// Add an unsigned offset to pointer using strict arithmetic. `count`
        /// is in units of bytes. The returned `PhysPtrMut<T>` has the same
        /// provenance as `self`.
        ///
        /// # Panics
        ///
        /// The function panics if the addition overflows, regardless of whether
        /// overflow checks are enabled.
        pub fn strict_byte_add(self, count: usize) -> PhysPtrMut<T> {
            // Calculate the checked physical address after addition.
            let addr = match self.phys_addr().checked_add(count) {
                Some(a) => a,
                None => panic!("strict_byte_add: addition overflow"),
            };

            // Create a physical pointer with addr and provenance of self.
            self.with_phys_addr(addr)
        }

        /// Subtracts an unsigned offset from the pointer using strict
        /// arithemetic. `count` is in units of `T`. The returned
        /// `PhysPtrMut<T>` has the same provenance as `self`.
        ///
        /// # Panics
        ///
        /// The function panics if the subtraction overflows, regardless of
        /// whether overflow checks are enabled.
        pub fn strict_sub(self, count: usize) -> PhysPtrMut<T> {
            // Calculate the byte offset.
            let byte_offset = match count.checked_mul(size_of::<T>()) {
                Some(o) => o,
                None => panic!("strict_sub: size_of::<T> * count overflows"),
            };

            // Calculate the checked physical address after subtraction.
            let addr = match self.phys_addr().checked_sub(byte_offset) {
                Some(a) => a,
                None => panic!("strict_sub: subtraction overflowed"),
            };

            self.with_phys_addr(addr)
        }

        /// Subtract an unsigned offset to pointer using strict arithmetic.
        /// `count` is in units of bytes. The returned `PhysPtrMut<T>` has the
        /// same provenance as `self`.
        ///
        /// # Panics
        ///
        /// The function panics if the subtraction overflows, regardless of
        /// whether overflow checks are enabled.
        pub fn strict_byte_sub(self, count: usize) -> PhysPtrMut<T> {
            // Calculate the checked physical address after subtraction.
            let addr = match self.phys_addr().checked_sub(count) {
                Some(a) => a,
                None => panic!("strict_byte_sub: subtraction overflow"),
            };

            // Create a physical pointer with addr and provenance of self.
            self.with_phys_addr(addr)
        }

        /// Checked addition of an unsigned offset to a pointer. `count` is in
        /// units of `T`. The returned `PhysPtrMut<T>` has the same provenance
        /// as `self`.
        ///
        /// Returns `None` if `count` after being scaled with `size_of::<T>` is
        /// too bit to fit in `usize` or if the addition of offset overflows
        /// the pointer address.
        ///
        /// The pointer returned by this is guaranteed to not have gone through
        /// wrapping behaviour.
        pub fn checked_add(self, count: usize) -> Option<PhysPtrMut<T>> {
            // Calculate the resulting address after addition of scaled count.
            let addr = self.phys_addr()
                .checked_add(count.checked_mul(size_of::<T>())?)?;

            Some(self.with_phys_addr(addr))
        }

        /// Checked addition of an unsigned offset to a pointer. `count` is in
        /// units of bytes. The returned `PhysPtrMut<T>` has the same provenance
        /// as `self`.
        ///
        /// Returns `None` if the addition of offset overflows the pointer
        /// address.
        ///
        /// The pointer returned by this is guaranteed to not have gone through
        /// wrapping behaviour.
        pub fn checked_byte_add(self, count: usize) -> Option<PhysPtrMut<T>> {
            let addr = self.phys_addr().checked_add(count)?;

            Some(self.with_phys_addr(addr))
        }

        /// Checked subtraction of an unsigned offset from a pointer. `count` is
        /// in units of `T`. The returned `PhysPtrMut<T>` has the same
        /// provenance as `self`.
        ///
        /// Returns `None` if `count` after being scaled with `size_of::<T>` is
        /// too bit to fit in `usize` or if the subtraction of offset overflows
        /// the pointer address.
        ///
        /// The pointer returned by this is guaranteed to not have gone through
        /// wrapping behaviour.
        pub fn checked_sub(self, count: usize) -> Option<PhysPtrMut<T>> {
            // Calculate the resulting address after addition of scaled count.
            let addr = self.phys_addr()
                .checked_sub(count.checked_mul(size_of::<T>())?)?;

            Some(self.with_phys_addr(addr))
        }

        /// Checked subtraction of an unsigned offset to a pointer. `count` is
        /// in units of bytes. The returned `PhysPtrMut<T>` has the same
        /// provenance as `self`.
        ///
        /// Returns `None` if the subtraction of offset overflows the
        /// pointer address.
        ///
        /// The pointer returned by this is guaranteed to not have gone through
        /// wrapping behaviour.
        pub fn checked_byte_sub(self, count: usize) -> Option<PhysPtrMut<T>> {
            let addr = self.phys_addr().checked_sub(count)?;

            Some(self.with_phys_addr(addr))
        }

        /// Performs a volatile read at the physical address pointed to by
        /// `PhysPtrMut<T>`. This is the only right way to read memory pointed
        /// to by a physical pointer.
        ///
        /// # Safety
        ///
        /// - `self` is properly aligned for `T`.
        /// - The physical address is valid for a volatile read on the target
        ///   hardware.
        /// - The read does not trap.
        /// - The read produces a properly initialized value of type `T`.
        /// - The access does not violates any hardware specific requirements.
        /// - The accessed memory is not Rust-allocated memory unless the usual
        ///   Rust rules for volatile access to Rust allocations are satisfied.
        ///
        /// Also read the safety preconditions of `core::ptr::read_volatile`.
        pub unsafe fn read_volatile(self) -> T {
            unsafe {
                self.as_ptr().read_volatile()
            }
        }

        /// Performs a volatile write at the physical address pointed to by
        /// `PhysPtrMut<T>`. This is the only write way to write to a physical
        /// address.
        ///
        /// # Safety
        ///
        /// - `self` is properly aligned for `T`, even for ZST.
        /// - The physical address is valid for a volatile write on the target
        ///   hardware.
        /// - The write does not trap.
        /// - The access does not violates any hardware specific requirements.
        /// - The accessed memory is not Rust-allocated memory unless the usual
        ///   Rust rules for volatile access to Rust allocations are satisfied.
        pub unsafe fn write_volatile(self, val: T) {
            unsafe {
                self.as_mut_ptr().write_volatile(val)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct PhysAddrRange {
    start: PhysAddr,
    len: usize,
}

impl PhysAddrRange {
    /// # Note
    ///
    /// This method of representing a range can never represent the range that
    /// contains all the address in the address space, i.e. starting from the
    /// beginning and stretching till the end. This is because `usize` could not
    /// store the number of possible states a `usize` could be in.
    fn with_start_and_len(start: PhysAddr, len: usize) -> Option<PhysAddrRange> {
        // If the range wraps around the address space then return none.
        let _end = start.checked_add(len.saturating_sub(1))?;

        Some(Self {
            start,
            len,
        })
    }

    fn with_start_and_end(start: PhysAddr, end: PhysAddr) -> Option<PhysAddrRange> {
        let len = end.addr().checked_sub(start.addr())?.checked_add(1)?;

        Some(Self {
            start,
            len,
        })
    }

    fn len(&self) -> usize {
        self.len
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn start(&self) -> PhysAddr {
        self.start
    }

    // Does not exist for an empty region.
    fn end(&self) -> Option<PhysAddr> {
        // If length is zero then it returns None.
        Some(self.start.strict_add(self.len.checked_sub(1)?))
    }

    fn contains(&self, other: &Self) -> bool {
        self.start() <= other.start() 
            && self.end() >= other.end()
    }

    fn overlap_with(&self, other: &Self) -> Overlap {
        if self.is_empty() || other.is_empty() {
            return Overlap::Empty;
        }

        if self == other {
            return Overlap::Equal;
        }

        let self_end = self.start.strict_add(self.len.strict_sub(1));
        let other_end = other.start.strict_add(other.len.strict_sub(1));

        if self_end < other.start() || other_end < self.start() {
            return Overlap::Empty;
        }

        if self.contains(other) {
            return Overlap::Envelop(*other);
        } else if other.contains(self) {
            return Overlap::Embedded(*self);
        }

        if self.start() < other_end && self_end >= other.start() {
            let common = Self {
                start: other.start(),
                len: self_end.addr().strict_sub(other.start().addr())
                    .strict_add(1),
            };

            return Overlap::Upper(common);
        } else if other.start() < self_end && other_end >= self.start() {
            let common = Self {
                start: self.start(),
                len: other_end.addr().strict_sub(self.start().addr())
                    .strict_add(1),
            };

            return Overlap::Lower(common);
        }

        unreachable!();
    }
}

/// Different types of overlapping behaviors that are possible between two
/// `PhysAddrRange`'s.
///
/// The fields are from the perspective of `self`, whose overlap is being
/// checked with `other`.
pub enum Overlap {
    /// No overlap between `self` and `other`.
    Empty,

    /// Higher address of `self` overlap with lower addresses of `other`.
    Upper(PhysAddrRange),

    /// Lower addresses of `self` overlap with higher addresses of `other`.
    Lower(PhysAddrRange),

    /// `self` contains other.
    Envelop(PhysAddrRange),

    /// `self` is contained in other.
    Embedded(PhysAddrRange),

    /// Both ranges are equal.
    Equal,
}
