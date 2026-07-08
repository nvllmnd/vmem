//! This module contains wrappers around [alloc::boxed::Box], [alloc::rc::Rc] and
//! [alloc::sync::Arc], each with [PageAllocator] as their [alloc::alloc::Allocator] type param
//!
//! These are for convience, each one derefs to its inner [Vmem], which can be used to implement a
//! custom allocator or for allocating regions of memory aligned to page size
//!
//! NOTE: These types DO NOT support [crate::os::vremap], as im not sure how to get that to play
//! nicely with the standard (core) library, if you want to remap (which is a whole other bucket of
//! worms), youll have to deal with the low-level [crate::os::valloc], [crate::os::vremap], and
//! [crate::os::vdestroy] and deal with the safety contracts of what all that entails yourself.
//! (i.e. not accessing old mappings after remap, if moved, ect)
//!
//!
use core::{ops::Deref, ptr::NonNull};

use alloc::{boxed::Box, rc::Rc, sync::Arc};

use crate::{page::PageAllocator, virt::Vmem};

#[repr(transparent)]
#[derive(Debug)]
pub struct Vbox(Box<Vmem, PageAllocator>);

impl Vbox {
    pub fn new(size: usize) -> Self {
        let b = Box::<[u8], PageAllocator>::new_zeroed_slice_in(size, PageAllocator);
        let b = unsafe { b.assume_init() };
        let (b, alloc) = Box::into_non_null_with_allocator(b);
        let b = unsafe {
            let b = NonNull::new_unchecked(b.as_ptr() as *mut Vmem);
            Box::from_non_null_in(b, alloc)
        };
        Self(b)
    }
}

impl Deref for Vbox {
    type Target = Vmem;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct Vrc(Rc<Vmem, PageAllocator>);

impl Vrc {
    pub fn new(size: usize) -> Self {
        let b = Rc::<[u8], PageAllocator>::new_zeroed_slice_in(size, PageAllocator);
        let b = unsafe { b.assume_init() };
        let (b, alloc) = Rc::into_raw_with_allocator(b);
        let b = unsafe { Rc::from_raw_in(b as *const Vmem, alloc) };
        Self(b)
    }
}
impl Deref for Vrc {
    type Target = Vmem;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl Clone for Vrc {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct Varc(Arc<Vmem, PageAllocator>);

impl Varc {
    pub fn new(size: usize) -> Self {
        let b = Arc::<[u8], PageAllocator>::new_zeroed_slice_in(size, PageAllocator);
        let b = unsafe { b.assume_init() };
        let (b, alloc) = Arc::into_raw_with_allocator(b);
        let b = unsafe { Arc::from_raw_in(b as *const Vmem, alloc) };
        Self(b)
    }
}
impl Deref for Varc {
    type Target = Vmem;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl Clone for Varc {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
