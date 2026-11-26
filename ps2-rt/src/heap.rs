//! A minimal global allocator, to unblock next-steps.md #4 ("pick the
//! second tool to port... a reasonable second target is something that
//! exercises `alloc`").
//!
//! Deliberately a bump allocator (never reclaims individual
//! allocations) rather than a freelist: next-steps.md explicitly called
//! that out as the simpler acceptable option, and nothing ported so far
//! needs `dealloc` to actually do anything. Backed by a fixed-size
//! static array rather than carving a region out of `link.ld` between
//! `_heap_start` and `_stack_top` -- that avoids having to decide how
//! much of the remaining RAM is safe to give the heap vs. leave as
//! stack headroom, which isn't something to guess at.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

const HEAP_SIZE: usize = 64 * 1024;

#[repr(align(16))]
struct HeapStorage(UnsafeCell<[u8; HEAP_SIZE]>);

// Safety: the EE here runs single-threaded, no-std, no interrupts enabled
// by anything in ps2-rt -- there is no concurrent access to race on.
unsafe impl Sync for HeapStorage {}

static HEAP: HeapStorage = HeapStorage(UnsafeCell::new([0; HEAP_SIZE]));

pub struct BumpAllocator {
    /// Byte offset of the next free byte within `HEAP`, before alignment.
    next: UnsafeCell<usize>,
}

// Safety: same as HeapStorage -- single-threaded target, no interior
// mutability races possible.
unsafe impl Sync for BumpAllocator {}

impl BumpAllocator {
    const fn new() -> Self {
        Self {
            next: UnsafeCell::new(0),
        }
    }
}

const fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let heap_start = HEAP.0.get() as usize;
        let next = self.next.get();
        let candidate = align_up(heap_start + *next, layout.align());
        let new_offset_end = candidate + layout.size() - heap_start;
        if new_offset_end > HEAP_SIZE {
            return core::ptr::null_mut();
        }
        *next = new_offset_end;
        candidate as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: individual allocations are never reclaimed.
    }
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator::new();

const _: () = assert!(align_up(0, 4) == 0);
const _: () = assert!(align_up(1, 4) == 4);
const _: () = assert!(align_up(4, 4) == 4);
const _: () = assert!(align_up(5, 8) == 8);
const _: () = assert!(align_up(0, 1) == 0);
