//! This module contains implementations for the [core::alloc::Allocator] and
//! [core::alloc::GlobalAlloc] traits
//!

use core::sync::atomic::AtomicU64;

use alloc::alloc::Allocator;

use crate::virt::Vmem;

#[derive(Debug)]
#[repr(C)]
pub struct Arena {
    mem: Vmem,
    used: AtomicU64,
}

unsafe impl Allocator for Arena {
    fn allocate(
        &self,
        layout: core::alloc::Layout,
    ) -> Result<core::ptr::NonNull<[u8]>, alloc::alloc::AllocError> {
        todo!()
    }

    unsafe fn deallocate(&self, ptr: core::ptr::NonNull<u8>, layout: core::alloc::Layout) {
        todo!()
    }
}
