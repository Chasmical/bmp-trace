pub(crate) fn alloc_zeroed<T: Default>(len: usize) -> Result<Box<[T]>, ()> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).or(Err(()))?;

    for slot in v.spare_capacity_mut() {
        slot.write(T::default());
    }

    Ok(v.into_boxed_slice())
}
