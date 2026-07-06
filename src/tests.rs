use crate::{os::megabytes, virt::Vmem};

#[test]
fn can_create_vmem() -> anyhow::Result<()> {
    {
        let _ = Vmem::new(megabytes(12));
    }
    Ok(())
}
