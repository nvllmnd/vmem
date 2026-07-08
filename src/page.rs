use core::{alloc::Layout, ptr::NonNull};

use alloc::alloc::Allocator;

use crate::os::{valloc_ex, vdestroy, vremap};

/// @brief deals with blocks of memory with minimum size of [crate::os::page_size] bytes
/// @details this type does not impl [alloc::alloc::GlobalAlloc], as that seems a little absurd, as
/// any allocation requests made smaller than page size would get rounded up to page size, wasting a
/// lot of space
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageAllocator;

impl PageAllocator {
    pub const fn new() -> Self {
        Self
    }

    /// # SAFETY
    /// @see [valloc_ex]
    pub unsafe fn alloc(&self, layout: Layout) -> anyhow::Result<NonNull<[u8]>> {
        unsafe { valloc_ex(layout.size(), crate::os::VmapMode::PrivateAnon) }
    }

    /// # SAFETY
    /// @see [vdestroy]
    pub unsafe fn free(&self, ptr: NonNull<[u8]>) -> anyhow::Result<()> {
        unsafe { vdestroy(ptr) }
    }

    /// # SAFETY
    /// @see [vremap]
    pub unsafe fn resize(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, alloc::alloc::AllocError> {
        let ptr = NonNull::slice_from_raw_parts(ptr, old_layout.size());
        let Some(ptr) =
            (unsafe { vremap(ptr, new_layout.size(), crate::os::RemapMode::AllowMove) })
        else {
            return Ok(ptr);
        };
        Ok(ptr)
    }
}

unsafe impl Allocator for PageAllocator {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        self.allocate_zeroed(layout)
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        unsafe {
            let ptr = NonNull::slice_from_raw_parts(ptr, layout.size());
            vdestroy(ptr).expect("Virtual memory should be unmapped without error! check error is aligned to page boundary and that layout is a valid size!");
        };
    }

    fn allocate_zeroed(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        let Ok(ptr) = (unsafe { valloc_ex(layout.size(), crate::os::VmapMode::PrivateAnon) })
        else {
            return Result::Err(alloc::alloc::AllocError);
        };

        Ok(ptr)
    }

    unsafe fn grow(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );
        unsafe { self.grow_zeroed(ptr, old_layout, new_layout) }
    }

    unsafe fn grow_zeroed(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        unsafe { self.resize(ptr, old_layout, new_layout) }
    }

    unsafe fn shrink(
        &self,
        ptr: core::ptr::NonNull<u8>,
        old_layout: core::alloc::Layout,
        new_layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );
        unsafe { self.resize(ptr, old_layout, new_layout) }
    }
}
