//!
//!This module contains a simple arena implementation.
//!
//!Uses [PageAllocator] as a default backing allocator
//!
//!

use core::{alloc::Layout, cell::Cell, ptr::NonNull};

use alloc::{alloc::Allocator, rc::Rc};
use anyhow::ensure;

use crate::{page::PageAllocator, virt::Vmem};

#[repr(C)]
#[derive(Debug)]
struct ArenaInner {
    used: Cell<u64>,
    mem: Vmem,
}

impl ArenaInner {
    pub const fn used_bytes(&self) -> u64 {
        self.used.get()
    }

    pub const fn vmem(&self) -> &Vmem {
        &self.mem
    }
}

/// @brief a bump-style Arena Allocator
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Arena<A: Allocator = PageAllocator>(Rc<ArenaInner, A>);

// where
//     A: Allocator + Clone,
// {
//     fn clone(&self) -> Self {
//         Self {
//             mem: Rc::clone(&self.mem),
//             used: Cell::new(self.used_bytes() as u64),
//         }
//     }
// }

impl Arena<PageAllocator> {
    #[inline]
    pub fn new(size_bytes: usize) -> Self {
        Self::new_in(size_bytes, PageAllocator)
    }
}

impl<A> Arena<A>
where
    A: Allocator,
{
    pub fn new_in(size_bytes: usize, alloc: A) -> Self {
        let mem = Rc::<[u8], A>::new_zeroed_slice_in(size_bytes, alloc);
        let (mem, alloc) = Rc::into_raw_with_allocator(mem);
        let mem = mem as *mut ArenaInner;
        unsafe { core::ptr::write(&raw mut (*mem).used, Cell::new(0)) };
        let mem = unsafe { Rc::from_raw_in(mem as *const _, alloc) };

        Self(mem)
    }

    pub fn resize(&self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> bool {
        let top = self.top_aligned(new_layout.align()).unwrap();
        let last = unsafe { top.sub(old_layout.size()) };
        assert!(last >= self.0.vmem().begin_ptr());
        if last == ptr {
            let delta = new_layout.size() as isize - old_layout.size() as isize;
            let result = self.used_bytes() as isize + delta;
            assert!(result >= 0);
            self.0.used.set(result as u64);
            true
        } else {
            false
        }
    }

    #[inline(always)]
    pub fn used_bytes(&self) -> usize {
        self.0.used_bytes() as usize
    }

    #[inline]
    pub fn allocator(&self) -> &A {
        Rc::allocator(&self.0)
    }

    fn top_aligned(&self, align: usize) -> anyhow::Result<NonNull<u8>> {
        let used = self.used_bytes();
        let ptr = unsafe { self.0.vmem().as_ptr().cast::<u8>().add(used) };
        ensure!(
            self.0.vmem().contains(ptr),
            "aligning pointer Arena top pointer to alignment: {align} by offset {used} creates a pointer that lies outside the range of this VirtMemory!"
        );
        let offset = ptr.align_offset(align);
        unsafe { Ok(ptr.add(offset)) }
    }
}

unsafe impl<A> Allocator for Arena<A>
where
    A: Allocator,
{
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        if layout.size() == 0 {
            return Ok(NonNull::slice_from_raw_parts(NonNull::dangling(), 0));
        }

        let Ok(ptr) = self.top_aligned(layout.align()) else {
            return core::result::Result::Err(alloc::alloc::AllocError);
        };

        let end = unsafe { ptr.add(layout.size()) };
        if end >= self.0.vmem().end_ptr() {
            return core::result::Result::Err(alloc::alloc::AllocError);
        }

        let size = end.addr().get() as isize - ptr.addr().get() as isize;
        assert!(size >= 0);

        let res = self.used_bytes() + size as usize;
        self.0.used.set(res as u64);

        let ptr = NonNull::slice_from_raw_parts(ptr, layout.size());
        Ok(ptr)
    }

    unsafe fn deallocate(&self, _ptr: core::ptr::NonNull<u8>, _layout: core::alloc::Layout) {}

    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<NonNull<[u8]>, alloc::alloc::AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        if self.resize(ptr, old_layout, new_layout) {
            Ok(NonNull::slice_from_raw_parts(ptr, new_layout.size()))
        } else {
            let new_ptr = self.allocate(new_layout)?;
            unsafe {
                NonNull::copy_from_nonoverlapping(
                    new_ptr.cast::<u8>(),
                    ptr.cast::<u8>(),
                    old_layout.size(),
                )
            };
            let delta = new_layout.size() - old_layout.size();
            self.0.used.set(self.used_bytes() as u64 + delta as u64);
            Ok(new_ptr)
        }
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<NonNull<[u8]>, alloc::alloc::AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        unsafe {
            let new_ptr = self.grow(ptr, old_layout, new_layout)?;
            let np = new_ptr.cast::<u8>().add(old_layout.size());
            NonNull::write_bytes(np, 0, new_layout.size() - old_layout.size());
            Ok(new_ptr)
        }
    }

    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<NonNull<[u8]>, alloc::alloc::AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );
        let top = self.top_aligned(new_layout.align()).unwrap();
        let last = unsafe { top.sub(old_layout.size()) };
        assert!(last >= self.0.vmem().begin_ptr());
        if last == ptr {
            let delta = new_layout.size() as isize - old_layout.size() as isize;
            let result = core::cmp::max(0, self.used_bytes() as isize + delta);
            self.0.used.set(result as u64);
        }
        Ok(NonNull::slice_from_raw_parts(ptr, new_layout.size()))
    }
}
