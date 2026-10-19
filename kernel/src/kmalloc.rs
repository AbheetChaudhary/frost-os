#![allow(unused)]

use core::{
    alloc::{
        Allocator,
        AllocatorNightly,
        AllocError,
        Layout,
    },
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr::NonNull,
};

/// Size of the block header.
const HEADER_SIZE: usize = 16;

fn align16(size: usize) -> usize {
    let has_rem = (size % 16) != 0;

    if !has_rem {
        size
    } else {
        (size & !((1<<4) - 1)).strict_add(1<<4)
    }
}

#[derive(Clone, Copy)]
#[repr(C, align(16))]
struct Header {
    next: *mut Header,
    prev: *mut Header,

    block: *mut u8,
    block_size: usize,
}

const _: () = {
    assert!(size_of::<Header>() == HEADER_SIZE);
    ()
};

impl Header {
    // requested_size must be 16 bytes aligned.
    pub fn should_split(&self, requested_size: usize) -> bool {
        // We should not split the block if after handing over the requested
        // size there is not enough space to store atleast one aligned block
        // header and its corresponding block of atleast 16 bytes size.

        let after_use = self.block_size.strict_sub(requested_size);

        // Just a sanity check.
        assert!(self.block.addr() % 16 == 0);

        // If we split then the address of the new header will be 16 byte
        // aligned. We just need to check the next condition and no need to
        // worry about alignment of the new splitted header and block.
        after_use >= HEADER_SIZE + 16
    }
}

pub struct Kmalloc {
    buffer: *mut u8,
    buffer_size: usize,

    next_free: *mut Header,
    total_free: usize,
}

impl Kmalloc {
    pub unsafe fn init(buffer: *mut u8, buffer_size: usize) -> Kmalloc {
        // Align up the provided buffer pointer and update the buffer_size.
        // Panic if it could not be aligned.
        let align_offset = buffer.align_offset(HEADER_SIZE);

        if buffer_size <= align_offset {
            // Buffer size is too small to get an aligned pointer within it.
            panic!("unalignable buffer pointer");
        }

        // Aligned buffer pointer.
        let buffer = buffer.wrapping_add(align_offset);

        // Remaining buffer size after aligning the buffer pointer.
        let buffer_size = buffer_size.strict_sub(align_offset);

        // After aligning things there should be enough space to fit at least
        // one header.
        assert!(buffer_size > HEADER_SIZE, "buffer size too small after alignment \
            adjustments.");

        // Initialize the free list first node.
        let h1 = Header {
            next: core::ptr::null_mut(),
            prev: core::ptr::null_mut(),
            block: buffer.wrapping_add(HEADER_SIZE),
            block_size: buffer_size.strict_sub(HEADER_SIZE),
        };

        unsafe {
            buffer.cast::<Header>().write(h1);
        }

        Kmalloc {
            buffer,
            buffer_size,

            next_free: buffer.cast(),
            total_free: buffer_size.strict_sub(HEADER_SIZE),
        }
    }

    // First fit.
    fn find_free_first_fit(&self, size: usize) -> Option<*mut Header> {
        let mut next_free = self.next_free;

        loop {
            if next_free.is_null() {
                break None;
            }

            let header = unsafe { next_free.read() };

            if header.block_size >= size {
                break Some(next_free);
            }

            next_free = header.next;
        }
    }

    pub fn allocate(&mut self, size: usize) -> Result<NonNull<[u8]>, AllocError> {
        let size = align16(size);

        let next_free = match self.find_free_first_fit(size) {
            Some(ptr) => ptr,

            None => {
                // Coalesce and try again.
                self.coalesce_free_blocks();

                match self.find_free_first_fit(size) {
                    Some(ptr) => ptr,
                    None => return Err(AllocError),
                }
            }
        };

        // Found a free block that is big enough to allocate from.

        let header = unsafe { &mut *next_free };

        // Unlink the block.
        //
        // Split if necessary.
        if header.should_split(size) {
            let splitted: *mut MaybeUninit<Header> = header.block.wrapping_add(size).cast();

            assert!(splitted.addr() % 16 == 0);

            // Initialize the splitted block.
            let new_splitted_header = Header {
                next: header.next,
                prev: header.prev,
                block: splitted.wrapping_add(1).cast(),
                block_size: header.block_size.strict_sub(size).strict_sub(HEADER_SIZE),
            };

            unsafe {
                let splitted = &mut *splitted;
                splitted.write(new_splitted_header);
            }

            // Fix the header previous to the header we are going to return.
            if !header.prev.is_null() {
                let prev = unsafe { &mut *header.prev };

                // Sanity check!
                assert!(prev.next == next_free);

                prev.next = splitted.cast();

            } else {
                // We are splitting the last remaining free block. Modify
                // Kmalloc to point to the newly splitted block.

                self.next_free = splitted.cast();
            }

            if !header.next.is_null() {
                // Fix the header that is next to the header we are going to return.
                let next = unsafe { &mut *header.next };

                // Sanity check!
                assert!(next.prev == next_free);

                next.prev = splitted.cast();
            }

            // We are giving up the size bytes of allocation and HEADER_SIZE
            // extra bytes to create the header of the splitted block.
            self.total_free = self.total_free.strict_sub(size.strict_add(HEADER_SIZE));

            header.block_size = size;
        } else {
            // Fix the header previous to the header we are going to return.
            if !header.prev.is_null() {
                let prev = unsafe { &mut *header.prev };

                // Sanity check!
                assert!(prev.next == next_free);

                prev.next = header.next;

            } else {
                // We are fully allocating the first free block. Divert the free
                // list to the next block.

                self.next_free = header.next;
            }

            if !header.next.is_null() {
                // Fix the header that is next to the header we are going to return.
                let next = unsafe { &mut *header.next };

                // Sanity check!
                assert!(next.prev == next_free);

                next.prev = header.prev;
            }

            // We are handing over the entire free block.
            self.total_free = self.total_free.strict_sub(header.block_size);
        }

        header.next = core::ptr::null_mut();
        header.prev = core::ptr::null_mut();

        // This should never panic.
        let block = NonNull::new(header.block).expect("data corruption happened");

        // crate::serial!("allocate({size:#x}): {:p}\n", block.as_ptr());
        let ret = NonNull::slice_from_raw_parts(block, size);

        Ok(ret)
    }

    pub unsafe fn deallocate(&mut self, ptr: NonNull<u8>, layout: Layout) {
        let header_ptr = ptr.as_ptr().cast::<Header>().wrapping_sub(1);

        let header = unsafe { &mut *header_ptr };

        assert!(header.next == core::ptr::null_mut());
        assert!(header.prev == core::ptr::null_mut());
        assert!(header.block == ptr.as_ptr());
        assert!(header.block_size >= layout.size());

        assert!(header_ptr.addr() != self.next_free.addr());

        if header_ptr.addr() < self.next_free.addr() || self.next_free.is_null() {
            header.next = self.next_free;
            self.next_free = header_ptr;

            if !header.next.is_null() {
                let next = unsafe { &mut *header.next };
                next.prev = header_ptr;
            }
        } else {
            // self.next_free will not change. We just need to fit the
            // header_ptr somewhere.

            let mut curr = self.next_free;
            let mut curr_header = unsafe { &mut *curr };

            while !curr_header.next.is_null() &&
                curr_header.next.addr() < header_ptr.addr() {

                curr = curr_header.next;
                curr_header = unsafe { &mut *curr };
            }

            if !curr_header.next.is_null() {
                // header_ptr will go between curr and next of curr.
                header.next = curr_header.next;
                header.prev = curr;

                curr_header.next = header_ptr;

                let next = unsafe { &mut *header.next };

                next.prev = header_ptr;
            } else {
                // header_ptr is at a higher address then any other free
                // headers.

                curr_header.next = header_ptr;
                header.prev = curr;
            }
        }

        // Despite an empty free list total_free might not be zero. So it is
        // better to add here, rather than an assignment of
        // header.block_size.
        self.total_free += header.block_size;

        // crate::serial!("deallocate({:p}), total_free: {:#x}\n", ptr.as_ptr(), self.total_free);
    }

    fn coalesce_free_blocks(&mut self) {
        if self.next_free.is_null() {
            return;
        }

        let first_header = unsafe { &*self.next_free };

        // Pointer to the current header.
        let mut curr = first_header.next;

        // Pointer to the header previous of current header.
        let mut prev = self.next_free;

        while !curr.is_null() {
            unsafe {
                // Sanity check.
                assert_eq!((&*curr).prev, prev);
            }

            let prev_header = unsafe { &mut *prev };
            let curr_header = unsafe { &*curr };

            // Sanity check.
            assert!(prev_header.next.addr() == curr.addr());

            // Check if previous and current blocks are adjacent.
            if prev_header.block.wrapping_add(prev_header.block_size).addr() ==
                curr.addr() {

                // Merge both blocks. Delete `curr` header, `prev` remains.
                //
                // Update the merged blocks' size and the total free size.

                prev_header.next = curr_header.next;
                prev_header.block_size = prev_header.block_size
                    .strict_add(curr_header.block_size).strict_add(HEADER_SIZE);

                self.total_free = self.total_free.strict_add(HEADER_SIZE);

                if !curr_header.next.is_null() {
                    let next_header = unsafe { &mut *curr_header.next };
                    next_header.prev = prev;
                }

                // prev stays prev, but curr advances.
                curr = curr_header.next;
            } else {
                prev = curr;
                curr = curr_header.next;
            }
        }
    }
}

pub struct CpuHeap {
    kmalloc: UnsafeCell<Kmalloc>,
}

unsafe impl Allocator for CpuHeap {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        assert!(layout.align() <= 16);
        assert!(layout.size() > 0, "Zero sized allocations are not allowed.");

        let size = align16(layout.size());

        let kmalloc = unsafe { &mut *self.kmalloc.get() };

        kmalloc.allocate(size)
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        let kmalloc = unsafe { &mut *self.kmalloc.get() };

        unsafe { kmalloc.deallocate(ptr, layout) };
    }
}

impl AllocatorNightly for CpuHeap {}

impl CpuHeap {
    pub unsafe fn init(ptr: *mut u8, size: usize) -> CpuHeap {
        assert!(ptr.addr() % (1<<12) == 0);
        assert!(size % (1<<12) == 0);

        let kmalloc = unsafe { Kmalloc::init(ptr, size) };

        Self {
            kmalloc: UnsafeCell::new(kmalloc),
        }
    }
}

/*
impl Drop for CpuHeap {
    fn drop(&mut self) {
        let slice_ptr: *mut [u8] = core::ptr::slice_from_raw_parts_mut(self.box_ptr,
            self.box_size);

        let b = unsafe { Box::from_raw(slice_ptr) };

        drop(b)
    }
}
*/
