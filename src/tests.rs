use alloc::{boxed::Box, vec::Vec};

use crate::{
    arena::Arena,
    os::{gigabytes, kilobytes, megabytes},
    ptr::Vbox,
    virt::VmemBox,
};

#[test]
fn can_create_vmem() -> anyhow::Result<()> {
    {
        let _ = VmemBox::new(megabytes(12));
    }
    Ok(())
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Point {
    x: i32,
    y: i32,
    z: i32,
    w: i32,
}

#[test]
fn arena_works() -> anyhow::Result<()> {
    let a = Arena::new(megabytes(28));
    let mut b = Box::new_in(Point::default(), a.clone());
    b.x = 500;
    b.y = 100;
    b.z = 300;
    b.w = 5;

    assert_eq!(b.x, 500);
    assert_eq!(b.y, 100);
    assert_eq!(b.z, 300);
    assert_eq!(b.w, 5);

    let mut v: Vec<f32, Arena> = alloc::vec::Vec::with_capacity_in(100, a.clone());
    v.resize(100, 0.0);

    (0..100).for_each(|x| {
        v[x] = (x * x) as f32;
    });

    (0..100).for_each(|x| {
        assert_eq!(v[x], (x * x) as f32);
    });

    Ok(())
}

#[test]
fn vmem_lock_and_prepop() -> anyhow::Result<()> {
    let vm = Vbox::new(gigabytes(4));
    vm.prepopulate(vm.begin_ptr(), kilobytes(16))?;
    vm.mlock(vm.begin_ptr(), kilobytes(24))?;
    vm.munlock(vm.begin_ptr(), kilobytes(12))?;
    Ok(())
}

#[test]
fn vmem_free_region() -> anyhow::Result<()> {
    let vm = Vbox::new(megabytes(24));
    let p = unsafe { vm.begin_ptr().add(megabytes(20)) };
    unsafe { vm.free_region(p, megabytes(3))? };
    Ok(())
}
