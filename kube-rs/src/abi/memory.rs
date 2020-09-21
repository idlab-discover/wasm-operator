use safe_transmute::{transmute_one, transmute_to_bytes};
use std::ffi::c_void;
use std::mem;

/// Struct to pass a pointer and its size to/from the host
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Ptr {
    pub(crate) ptr: u32,
    pub(crate) size: u32,
}
unsafe impl safe_transmute::TriviallyTransmutable for Ptr {}

impl From<u64> for Ptr {
    fn from(value: u64) -> Self {
        transmute_one(transmute_to_bytes(&[value])).unwrap()
    }
}

#[no_mangle]
pub extern "C" fn allocate(size: usize) -> *mut c_void {
    let mut buffer = Vec::with_capacity(size);
    let pointer = buffer.as_mut_ptr();
    // Say to compiler to forget about this memory cell
    // Deallocation will be done by who's going to consume this allocation
    mem::forget(buffer);

    pointer as *mut c_void
}