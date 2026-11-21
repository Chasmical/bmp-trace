use std::mem::MaybeUninit;

pub(crate) fn alloc_uninit_slice<T>(len: usize) -> Result<Box<[MaybeUninit<T>]>, ()> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).or(Err(()))?;
    unsafe { v.set_len(len) };
    Ok(v.into_boxed_slice())
}

pub(crate) fn alloc_default_slice<T: Default + Copy>(len: usize) -> Result<Box<[T]>, ()> {
    let mut slice = alloc_uninit_slice::<T>(len)?;
    slice.fill(MaybeUninit::new(T::default()));
    Ok(unsafe { slice.assume_init() })
}
