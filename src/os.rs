//!
//! This module contains low level functions that wrap POSIX [libc::mmap], [libc::mremap], and
//! [libc::munmap]
//!
//!

use core::ptr::{NonNull, null_mut};

use alloc::rc::Rc;
use anyhow::{bail, ensure};

// TODO: Implement more flags compatible with private anonymous memory mappings
//
/// @brief configures flags passed to [libc::mmap]
/// @details default value is just [libc::MAP_PRIVATE] | [libc::MAP_ANONYMOUS], or the same behavior as a call to [valloc]
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VmapMode {
    #[default]
    PrivateAnon,
    /// @brief implies [VmapMode::PrivateAnon] as well
    NoReserve,
}

/// @brief uses [libc::mmap] to allocate a new region of virtual memory
/// @details
///  # SAFETY
///  Caller is expected to free returned raw pointer by calling [VirtMemory::destroy] or
///  [vdestroy]
pub unsafe fn valloc(size_bytes: usize) -> anyhow::Result<NonNull<[u8]>> {
    unsafe { valloc_ex(size_bytes, VmapMode::default()) }
}

///  # SAFETY
///  Caller is expected to free returned raw pointer by calling [VirtMemory::destroy] or
///  [vdestroy]
pub unsafe fn valloc_ex(size_bytes: usize, mode: VmapMode) -> anyhow::Result<NonNull<[u8]>> {
    let size = core::cmp::max(size_bytes, page_size());
    let flags = match mode {
        VmapMode::PrivateAnon => libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
        VmapMode::NoReserve => libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
    };
    let mem = unsafe {
        match libc::mmap(
            null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            flags,
            -1,
            0,
        ) {
            libc::MAP_FAILED => {
                bail!("Failed to create virtual memory of size: {size}");
            }
            ptr => {
                ensure!(
                    !ptr.is_null(),
                    "pointer returned by mmap should not be null!"
                );

                let ptr: NonNull<u8> = NonNull::new_unchecked(ptr as *mut _);
                NonNull::slice_from_raw_parts(ptr, size)
            }
        }
    };
    Ok(mem)
}

///  @brief wraps call to [libc::munmap]
///  # SAFETY
///  Caller is expected to free only pass a raw pointer that was returned by [valloc] (or
///  [VirtMemory::alloc])
pub unsafe fn vdestroy(ptr: NonNull<[u8]>) -> anyhow::Result<()> {
    unsafe {
        let size = ptr.len();
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
/// [RemapMode::AllowMove]
pub unsafe fn vremap(
    ptr: NonNull<[u8]>,
    new_size: usize,
    mode: RemapMode,
) -> Option<NonNull<[u8]>> {
    let flags = match mode {
        RemapMode::ResizeInPlace => 0,
        RemapMode::AllowMove => libc::MREMAP_MAYMOVE,
    };

    let ptr = unsafe {
        let old_size = ptr.len();
        let ptr = libc::mremap(ptr.as_ptr() as *mut _, old_size, new_size, flags);
        if ptr == libc::MAP_FAILED {
            return None;
        }
        assert!(!ptr.is_null());

        let ptr = NonNull::new_unchecked(ptr as *mut _).cast::<u8>();
        NonNull::slice_from_raw_parts(ptr, new_size)
        // let ptr = NonNull::new_unchecked(ptr.as_ptr() as *mut Vmem);
        // (*ptr.as_ptr()).size_bytes.set(new_size as u64);
        // ptr
    };
    Some(ptr)
}

pub fn page_size() -> usize {
    // NOTE: here we cache SIZE so that we only have to make a syscall once
    static mut SIZE: usize = 0;

    let isinit = unsafe { SIZE } != 0;

    if !isinit {
        let s = unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) };

        assert!(s > 0);
        unsafe { SIZE = s as usize };
    }
    unsafe { SIZE }
}

extern crate alloc;

pub const KB1: usize = 1024;

pub const fn kilobytes(n: usize) -> usize {
    n * KB1
}

pub const fn megabytes(n: usize) -> usize {
    kilobytes(n) * KB1
}

pub const fn gigabytes(n: usize) -> usize {
    megabytes(n) * KB1
}

pub const fn terabytes(n: usize) -> usize {
    gigabytes(n) * KB1
}
