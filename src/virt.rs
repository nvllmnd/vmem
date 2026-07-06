//! This module contains types abstracting operating system virtual memory
use core::{
    cell::UnsafeCell,
    ops::Deref,
    ptr::{NonNull, null_mut},
    sync::atomic::AtomicU64,
};

use anyhow::{bail, ensure};

use crate::os::*;

/// @brief a block of virtual operating system virtual memory
/// @detail allocated by a call to [libc::mmap] and deallocated by [libc::munmap]
#[repr(C)]
#[derive(Debug)]
pub struct VirtMemory {
    size_bytes: AtomicU64,
    data: UnsafeCell<[u8]>,
}

impl VirtMemory {
    /// @brief uses [libc::mmap] to allocate a new region of virtual memory
    /// @details
    /// # SAFETY
    /// Caller is expected to free returned raw pointer by calling [VirtMemory::destroy] or [vdestroy])
    /// This function forwards call to [valloc]
    #[inline(always)]
    pub unsafe fn alloc(size_bytes: usize) -> anyhow::Result<NonNull<Self>> {
        unsafe { valloc(size_bytes) }
    }

    /// @brief forwards call to [vdestroy]
    /// @details
    ///  # SAFETY
    ///  Caller is expected to free returned raw pointer
    #[inline(always)]
    pub unsafe fn destroy(s: NonNull<Self>) -> anyhow::Result<()> {
        unsafe { vdestroy(s) }
    }

    pub const fn as_slice(&self) -> &[u8] {
        unsafe { &(*self.data.get()) }
    }

    #[inline]
    pub fn size_bytes(&self) -> usize {
        self.size_bytes.load(core::sync::atomic::Ordering::Relaxed) as usize
    }

    /// @brief wrapper around [libc::mlock]
    /// @returns [anyhow::Result::Err] if given ptr is outside the range of this [VirtMemory],
    /// or inner call to [libc::mlock] returns -1 (indicating error)
    pub fn mlock(&self, ptr: NonNull<u8>, size_bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.contains(ptr) && self.contains(unsafe { ptr.add(size_bytes) }),
            "given pointer is outside the range of this VirtMemory!"
        );
        let err = unsafe { libc::mlock(ptr.as_ptr() as *mut _, size_bytes) };
        ensure!(
            err != -1,
            "Failed to lock sub-region (of size: {size_bytes} bytes) of VirtMemory to physical RAM!"
        );
        Ok(())
    }

    /// @brief wrapper around [libc::munlock]
    /// @returns [anyhow::Result::Err] if given ptr is outside the range of this [VirtMemory],
    /// or inner call to [libc::munlock] returns -1 (indicating error)
    pub fn munlock(&self, ptr: NonNull<u8>, size_bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.contains(ptr) && self.contains(unsafe { ptr.add(size_bytes) }),
            "given pointer is outside the range of this VirtMemory!"
        );
        let err = unsafe { libc::munlock(ptr.as_ptr() as *mut _, size_bytes) };
        ensure!(
            err != -1,
            "Failed to unlock sub-region (of size: {size_bytes} bytes) of VirtMemory from physical RAM!"
        );
        Ok(())
    }

    /// @brief wraps call to [libc::madvise]
    /// @details forwards given arguments along with [libc::MADV_POPULATE_READ]
    /// @returns [anyhow::Result::Err] if given ptr is outside the range of this [VirtMemory],
    /// or inner call to [libc::advise] returns -1 (indicating error)
    pub fn prepopulate(&self, ptr: NonNull<u8>, size_bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.contains(ptr) && self.contains(unsafe { ptr.add(size_bytes) }),
            "given pointer is outside the range of this VirtMemory!"
        );

        let err =
            unsafe { libc::madvise(ptr.as_ptr() as *mut _, size_bytes, libc::MADV_POPULATE_READ) };

        ensure!(
            err != -1,
            "Failed to prefault sub-region (of size: {size_bytes} bytes) of VirtMemory! (call to madvise returned -1, indicating error!)"
        );

        Ok(())
    }

    /// @brief wrapper around [libc::madvise].
    /// @details , forwards  given arguments to it along with the advice: [libc::MADV_DONTNEED]
    ///
    /// # SAFETY
    /// Caller is responsible for ensuring that no pointers or borrowed references exist that
    /// point to / reference sub-region of [VirtMemory] in given range. Or that if there are
    /// some that exist, they are not dereferenced or read after this function returns. All
    /// references to memory in this sub-region of memory are invalidated after this function
    /// returns successfully
    pub unsafe fn free_region(&self, ptr: NonNull<u8>, size_bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.contains(ptr) && self.contains(unsafe { ptr.add(size_bytes) }),
            "given pointer is outside the range of this VirtMemory!"
        );

        let err = unsafe { libc::madvise(ptr.as_ptr() as *mut _, size_bytes, libc::MADV_DONTNEED) };

        ensure!(
            err != -1,
            "Failed to release sub-region (of size: {size_bytes} bytes) of VirtMemory back to operating system! (call to madvise returned -1, indicating error!)"
        );

        self.size_bytes
            .fetch_sub(size_bytes as u64, core::sync::atomic::Ordering::SeqCst);

        Ok(())
    }

    /// @brief non-null raw pointer to the start of virtual memory
    /// @details points to immediately after the field [VirtMemory::size_bytes] (not the member function!)
    /// for an end pointer that points to +1 past the end of this virtual memory, @see
    /// [VirtMemory::end_ptr]
    pub const fn begin_ptr(&self) -> NonNull<u8> {
        unsafe { NonNull::new_unchecked(self.data.get() as *mut _) }
    }

    /// @brief points to +1 past the end of this  [VirtMemory]
    pub fn end_ptr(&self) -> NonNull<u8> {
        unsafe { self.begin_ptr().add(self.size_bytes()) }
    }

    /// @brief checks given pointer is in the range of this [VirtMemory]
    #[inline]
    pub fn contains(&self, ptr: NonNull<u8>) -> bool {
        ptr >= self.begin_ptr() && ptr < self.end_ptr()
    }
}

/// @brief uses [libc::mmap] to allocate a new region of virtual memory
/// @details
///  # SAFETY
///  Caller is expected to free returned raw pointer by calling [VirtMemory::destroy] or
///  [vdestroy]
pub unsafe fn valloc(size_bytes: usize) -> anyhow::Result<NonNull<VirtMemory>> {
    let size = core::cmp::max(size_bytes, page_size());
    let mem = unsafe {
        match libc::mmap(
            null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
            -1,
            0,
        ) {
            libc::MAP_FAILED => {
                bail!("Failed to create virtual memory of size: {size_bytes}");
            }
            ptr => {
                ensure!(
                    !ptr.is_null(),
                    "pointer returned by mmap should not be null!"
                );

                let ptr: NonNull<u8> = NonNull::new_unchecked(ptr as *mut _);
                let ptr = NonNull::slice_from_raw_parts(ptr, size_bytes);
                let ptr = NonNull::new_unchecked(ptr.as_ptr() as *mut VirtMemory);
                (*ptr.as_ptr()).size_bytes = AtomicU64::new(size_bytes as u64);
                ptr
            }
        }
    };

    Ok(mem)
}

///  @brief wraps call to [libc::munmap]
///  # SAFETY
///  Caller is expected to free only pass a raw pointer that was returned by [valloc] (or
///  [VirtMemory::alloc])
pub unsafe fn vdestroy(ptr: NonNull<VirtMemory>) -> anyhow::Result<()> {
    unsafe {
        let size = (*ptr.as_ptr()).size_bytes();
        let err = libc::munmap(ptr.cast::<libc::c_void>().as_ptr(), size as libc::size_t);
        ensure!(
            err != -1,
            "munmap should not return -1! This indicates an error when freeing Vmem!"
        );
    }
    Ok(())
}

/// @brief used to pass to [vremap]
/// @details defaults to [RemapMode::ResizeInPlace]
#[derive(Debug, Clone, Copy, Default)]
pub enum RemapMode {
    /// @brief causes [vremap] to return [None] if operating system was unable to resize virtual
    /// memory region without moving it
    #[default]
    ResizeInPlace,
    AllowMove,
}

/// @brief resizes a currently allcoated [VirtMemory]
/// @details returns [None] if resize was not successful.
///
/// # SAFETY
/// The reason we are returning [Option] instead of [anyhow::Result], because failure is not
/// necessarily an error. Caller could pass [RemapMode::ResizeInPlace], and then call again with
/// [RemapMode::AllowMode]
pub unsafe fn vremap(
    ptr: NonNull<VirtMemory>,
    new_size: usize,
    mode: RemapMode,
) -> Option<NonNull<VirtMemory>> {
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
        let ptr = NonNull::new_unchecked(ptr.as_ptr() as *mut VirtMemory);
        (*ptr.as_ptr())
            .size_bytes
            .store(new_size as u64, core::sync::atomic::Ordering::SeqCst);
        ptr
    };
    Some(ptr)
}

/// @brief wrapper around a non-null [VirtMemory] raw pointer
/// @details for an owned version of this type that impl's [Drop], @see [Vmem]
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VmemRaw(NonNull<VirtMemory>);

impl VmemRaw {
    #[inline(always)]
    pub fn new(size: usize) -> Self {
        Self::try_new(size).expect("valloc should return without error!")
    }

    #[inline(always)]
    pub fn try_new(size: usize) -> anyhow::Result<Self> {
        let mem = unsafe { valloc(size)? };
        Ok(Self(mem))
    }

    #[inline(always)]
    pub fn destroy(self) -> anyhow::Result<()> {
        unsafe { vdestroy(self.0) }
    }

    /// @brief returns borrowed reference to inner [VirtMemory]
    /// # SAFETY
    /// This is a wrapper around a raw pointer derefernce, and as such, caller must be sure to
    /// follow Rusts aliasing requirements for raw poitners & references
    pub const unsafe fn as_ref(&self) -> &VirtMemory {
        unsafe { &(*self.0.as_ptr()) }
    }
}

/// @brief an owned region of private, anonymous virtual memory
#[repr(transparent)]
#[derive(Debug)]
pub struct Vmem(VmemRaw);

impl Vmem {
    #[inline(always)]
    pub fn new(size: usize) -> Self {
        Self(VmemRaw::new(size))
    }

    #[inline(always)]
    pub fn destroy(s: Self) -> anyhow::Result<()> {
        VmemRaw::destroy(s.0)
    }

    #[inline(always)]
    pub fn data(&self) -> &UnsafeCell<[u8]> {
        &self.data
    }

    /// @brief calls [vremap] on inner [VirtMemory] raw poitner
    /// # SAFETY
    /// This function is unsafe due to the fact that if [RemapMode::AllowMove] is passed, all
    /// pointers to any memory in this [VirtMemory] prior to this call are invalidated after this
    /// function returns
    ///
    /// If [RemapMode::]
    ///
    pub unsafe fn remap(self, new_size: usize, mode: RemapMode) -> Self {
        let Some(s) = (unsafe { vremap(self.0.0, new_size, mode) }) else {
            return self;
        };
        Self(VmemRaw(s))
    }
}

impl Deref for Vmem {
    type Target = VirtMemory;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &(*self.0.0.as_ptr()) }
    }
}

impl Drop for Vmem {
    #[inline]
    fn drop(&mut self) {
        VmemRaw::destroy(self.0)
            .expect("Virtual memory should be unmapped without any errors occurring!");
    }
}

unsafe impl Sync for Vmem {}
unsafe impl Sync for VirtMemory {}
unsafe impl Sync for VmemRaw {}
