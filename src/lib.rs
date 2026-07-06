#![no_std]
#![feature(allocator_api)]

extern crate alloc;

#[cfg(test)]
mod tests;

pub mod malloc;
pub mod virt;
pub mod os {

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
}

