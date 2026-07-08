//! This module contains types abstracting operating system virtual memory
//!
//! There is a custom box type around [Vmem] , [VmemBox]. we dont do [alloc::boxed::Box] with [Vmem]
//! and [crate::page::PageAllocator], as that restricts the ability to [vremap] on the box's inner
//! pointer.
//! If you don't require [libc::mremap] behavior, there are type aliases provided for that:[Vbox],
//! [VrcBox], and [VarcBox]
//!
//!
use core::{cell::UnsafeCell, ops::Deref, ptr::NonNull};

use alloc::{boxed::Box, rc::Rc, sync::Arc};
use anyhow::{bail, ensure};

use crate::{os::*, page::PageAllocator};

/// @brief a block of virtual operating system virtual memory
/// @detail allocated by a call to [libc::mmap] and deallocated by [libc::munmap]
#[repr(transparent)]
#[derive(Debug)]
pub struct Vmem(UnsafeCell<[u8]>);

impl Vmem {
    /// @brief uses [libc::mmap] to allocate a new region of virtual memory
    /// @details
    /// # SAFETY
    /// Caller is expected to free returned raw pointer by calling [Vmem::destroy] or [vdestroy])
    /// This function forwards call to [valloc]
    #[inline(always)]
    pub unsafe fn alloc(size_bytes: usize) -> anyhow::Result<NonNull<Self>> {
        let ptr = unsafe {
            let ptr = valloc_ex(size_bytes, VmapMode::default())?;
            NonNull::new_unchecked(ptr.as_ptr() as *mut Vmem)
        };
        Ok(ptr)
    }

    /// @brief forwards call to [vdestroy]
    /// @details
    ///  # SAFETY
    ///  Caller is expected to free returned raw pointer
    #[inline(always)]
    pub unsafe fn destroy(s: NonNull<Self>) -> anyhow::Result<()> {
        unsafe {
            let size = (*s.as_ptr()).size_bytes();
            let err = libc::munmap(s.as_ptr().cast::<libc::c_void>(), size as libc::size_t);
            ensure!(
                err != -1,
                "munmap should not return -1! This indicates an error when freeing Vmem!"
            );
        }
        Ok(())
    }

    /// @brief resizes a currently allcoated [Vmem]
    /// @details returns [None] if resize was not successful.
    ///
    /// # SAFETY
    /// The reason we are returning [Option] instead of [anyhow::Result], because failure is not
    /// necessarily an error. Caller could pass [RemapMode::ResizeInPlace], and then call again with
    /// [RemapMode::AllowMode]
    pub unsafe fn remap(
        ptr: NonNull<Self>,
        new_size: usize,
        mode: RemapMode,
    ) -> Option<NonNull<Self>> {
        let flags = match mode {
            RemapMode::ResizeInPlace => 0,
            RemapMode::AllowMove => libc::MREMAP_MAYMOVE,
        };

        let ptr = unsafe {
            let old_size = (*ptr.as_ptr()).size_bytes();
            let ptr = libc::mremap(ptr.as_ptr() as *mut _, old_size, new_size, flags);
            if ptr == libc::MAP_FAILED {
                return None;
            }
            assert!(!ptr.is_null());

            let ptr = NonNull::new_unchecked(ptr as *mut _).cast::<u8>();
            let ptr = NonNull::slice_from_raw_parts(ptr, new_size);
            let ptr = NonNull::new_unchecked(ptr.as_ptr() as *mut Vmem);
            ptr
        };
        Some(ptr)
    }

    pub const fn as_slice(&self) -> &[u8] {
        unsafe { &(*self.0.get()) }
    }

    pub const fn as_mut_slice(&mut self) -> &mut [u8] {
        self.0.get_mut()
    }

    /// # SAFETY
    /// Caller must ensure there are no references that point to the contents of inner data in order
    /// for this function to be safe
    #[allow(clippy::mut_from_ref)]
    pub const unsafe fn as_mut(&self) -> &mut [u8] {
        unsafe { &mut (*self.0.get()) }
    }

    pub const fn as_ptr(&self) -> NonNull<[u8]> {
        unsafe { NonNull::new_unchecked(self.0.get()) }
    }

    pub const fn size_bytes(&self) -> usize {
        unsafe { (&*self.0.get()).len() }
    }

    /// @brief wrapper around [libc::mlock]
    /// @returns [anyhow::Result::Err] if given ptr is outside the range of this [Vmem],
    /// or inner call to [libc::mlock] returns -1 (indicating error)
    pub fn mlock(&self, ptr: NonNull<u8>, size_bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.contains(ptr) && self.contains(unsafe { ptr.add(size_bytes) }),
            "given pointer is outside the range of this Vmem!"
        );
        let err = unsafe { libc::mlock(ptr.as_ptr() as *mut _, size_bytes) };
        ensure!(
            err != -1,
            "Failed to lock sub-region (of size: {size_bytes} bytes) of Vmem to physical RAM!"
        );
        Ok(())
    }

    /// @brief wrapper around [libc::munlock]
    /// @returns [anyhow::Result::Err] if given ptr is outside the range of this [Vmem],
    /// or inner call to [libc::munlock] returns -1 (indicating error)
    pub fn munlock(&self, ptr: NonNull<u8>, size_bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.contains(ptr) && self.contains(unsafe { ptr.add(size_bytes) }),
            "given pointer is outside the range of this Vmem!"
        );
        let err = unsafe { libc::munlock(ptr.as_ptr() as *mut _, size_bytes) };
        ensure!(
            err != -1,
            "Failed to unlock sub-region (of size: {size_bytes} bytes) of Vmem from physical RAM!"
        );
        Ok(())
    }

    /// @brief wraps call to [libc::madvise]
    /// @details forwards given arguments along with [libc::MADV_POPULATE_READ]
    /// @returns [anyhow::Result::Err] if given ptr is outside the range of this [Vmem],
    /// or inner call to [libc::advise] returns -1 (indicating error)
    pub fn prepopulate(&self, ptr: NonNull<u8>, size_bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.contains(ptr) && self.contains(unsafe { ptr.add(size_bytes) }),
            "given pointer is outside the range of this Vmem!"
        );

        let err =
            unsafe { libc::madvise(ptr.as_ptr() as *mut _, size_bytes, libc::MADV_POPULATE_READ) };

        ensure!(
            err != -1,
            "Failed to prefault sub-region (of size: {size_bytes} bytes) of Vmem! (call to madvise returned -1, indicating error!)"
        );

        Ok(())
    }

    /// @brief wrapper around [libc::madvise].
    /// @details , forwards  given arguments to it along with the advice: [libc::MADV_DONTNEED]
    ///
    /// # SAFETY
    /// Caller is responsible for ensuring that no pointers or borrowed references exist that
    /// point to / reference sub-region of [Vmem] in given range. Or that if there are
    /// some that exist, they are not dereferenced or read after this function returns. All
    /// references to memory in this sub-region of memory are invalidated after this function
    /// returns successfully
    pub unsafe fn free_region(&self, ptr: NonNull<u8>, size_bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.contains(ptr) && self.contains(unsafe { ptr.add(size_bytes) }),
            "given pointer is outside the range of this Vmem!"
        );

        let err = unsafe { libc::madvise(ptr.as_ptr() as *mut _, size_bytes, libc::MADV_DONTNEED) };

        ensure!(
            err != -1,
            "Failed to release sub-region (of size: {size_bytes} bytes) of Vmem back to operating system! (call to madvise returned -1, indicating error!)"
        );

        Ok(())
    }

    /// @brief non-null raw pointer to the start of virtual memory
    /// @details for an end pointer that points to +1 past the end of this virtual memory, @see
    /// [Vmem::end_ptr]
    pub const fn begin_ptr(&self) -> NonNull<u8> {
        unsafe { NonNull::new_unchecked(self.0.get() as *mut _) }
    }

    /// @brief points to +1 past the end of this  [Vmem]
    pub fn end_ptr(&self) -> NonNull<u8> {
        unsafe { self.begin_ptr().add(self.size_bytes()) }
    }

    /// @brief checks given pointer is in the range of this [Vmem]
    #[inline]
    pub fn contains(&self, ptr: NonNull<u8>) -> bool {
        ptr >= self.begin_ptr() && ptr < self.end_ptr()
    }

    ///
    /// @details for a safe version of this funciton,  @see [Vmem::copy_at_from_bytes]
    /// # SAFETY
    /// Caller must ensure there are no active references to the contents of inner data
    pub unsafe fn write_bytes_at(&self, offset: usize, bytes: &[u8]) -> anyhow::Result<()> {
        let end = offset + bytes.len();
        ensure!(end < self.size_bytes());
        unsafe {
            self.as_mut()[offset..end].copy_from_slice(bytes);
        }
        Ok(())
    }

    /// # SAFETY
    /// Caller must ensure there are no active references to the contents of inner data
    pub unsafe fn write_at<T>(&self, offset: usize, val: T) -> anyhow::Result<()>
    where
        T: bytemuck::Pod + bytemuck::Zeroable,
    {
        let bytes = bytemuck::bytes_of(&val);
        unsafe { self.write_bytes_at(offset, bytes) }
    }

    /// @brief copies given byte slice to offset.
    /// @details this is the safe version of [Vmem::write_bytes_at]
    pub fn copy_at_from_bytes(&mut self, offset: usize, bytes: &[u8]) -> anyhow::Result<()> {
        let end = offset + bytes.len();
        ensure!(end < self.size_bytes());
        {
            let sl = &mut self.as_mut_slice()[offset..end];

            assert!(sl.len() == bytes.len());

            sl.copy_from_slice(bytes);
        }

        Ok(())
    }

    pub fn read_as<T>(&self, offset: usize) -> anyhow::Result<&T>
    where
        T: bytemuck::Pod + bytemuck::Zeroable,
    {
        ensure!(offset < self.size_bytes());

        let ptr = unsafe { self.begin_ptr().add(offset) };
        let delta = ptr.align_offset(core::mem::align_of::<T>());
        let ptr = unsafe { ptr.add(delta) };
        let ptr = NonNull::slice_from_raw_parts(ptr, core::mem::size_of::<T>());
        unsafe { Ok(bytemuck::from_bytes(&(*ptr.as_ptr()))) }
    }

    pub fn read_as_slice<T>(&self, offset: usize, count: usize) -> anyhow::Result<&[T]>
    where
        T: bytemuck::Pod + bytemuck::Zeroable,
    {
        ensure!(offset < self.size_bytes());

        let ptr = unsafe { self.begin_ptr().add(offset) };
        let delta = ptr.align_offset(core::mem::align_of::<T>());
        let ptr = unsafe { ptr.add(delta) };
        let ptr = NonNull::slice_from_raw_parts(ptr, core::mem::size_of::<T>() * count);
        unsafe { Ok(bytemuck::cast_slice(&(*ptr.as_ptr()))) }
    }
}

/// @brief wrapper around a non-null [Vmem] raw pointer
/// @details for an owned version of this type that impl's [Drop], @see [Vmem]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VmemRaw(NonNull<Vmem>);
impl VmemRaw {
    #[inline(always)]
    pub fn new(size: usize) -> Self {
        Self::try_new(size).expect("valloc should return without error!")
    }

    #[inline(always)]
    pub fn try_new(size: usize) -> anyhow::Result<Self> {
        let mem = unsafe { Vmem::alloc(size)? };
        Ok(Self(mem))
    }

    #[inline(always)]
    pub fn destroy(self) -> anyhow::Result<()> {
        unsafe { Vmem::destroy(self.0) }
    }

    /// @brief returns borrowed reference to inner [Vmem]
    /// # SAFETY
    /// This is a wrapper around a raw pointer derefernce, and as such, caller must be sure to
    /// follow Rusts aliasing requirements for raw poitners & references
    pub const unsafe fn as_ref(&self) -> &Vmem {
        unsafe { &(*self.0.as_ptr()) }
    }
}

impl Deref for VmemRaw {
    type Target = Vmem;

    fn deref(&self) -> &Self::Target {
        unsafe { self.as_ref() }
    }
}

/// @brief an owned region of private, anonymous virtual memory
#[repr(transparent)]
#[derive(Debug)]
pub struct VmemBox(VmemRaw);

impl VmemBox {
    #[inline(always)]
    pub fn new(size: usize) -> Self {
        Self(VmemRaw::new(size))
    }

    #[inline(always)]
    pub fn destroy(s: Self) -> anyhow::Result<()> {
        VmemRaw::destroy(s.0)
    }

    pub const fn data(&self) -> &UnsafeCell<[u8]> {
        unsafe { &(*self.0.0.as_ptr()).0 }
    }

    /// @brief calls [vremap] on inner [Vmem] raw poitner
    /// # SAFETY
    /// This function is unsafe due to the fact that if [RemapMode::AllowMove] is passed, all
    /// pointers to any memory in this [Vmem] prior to this call are invalidated after this
    /// function returns
    ///
    /// If [RemapMode::]
    ///
    pub unsafe fn move_remap(self, new_size: usize, mode: RemapMode) -> Self {
        let Some(s) = (unsafe { Vmem::remap(self.0.0, new_size, mode) }) else {
            return self;
        };
        Self(VmemRaw(s))
    }
}

impl Deref for VmemBox {
    type Target = Vmem;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &(*self.0.0.as_ptr()) }
    }
}

impl Drop for VmemBox {
    #[inline]
    fn drop(&mut self) {
        VmemRaw::destroy(self.0)
            .expect("Virtual memory should be unmapped without any errors occurring!");
    }
}

// TODO: Write custom Arc for Vmem so we can be more confident in its concurrency support
//
// struct VmemArcInner {
//     strong: AtomicI64,
//     inner: Vmem,
// }
//
// pub struct VmemArc(NonNull<VmemArcInner>);
//
unsafe impl Sync for VmemBox {}
unsafe impl Sync for Vmem {}
unsafe impl Sync for VmemRaw {}
