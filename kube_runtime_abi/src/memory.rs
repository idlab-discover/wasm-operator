use std::ffi::c_void;
use std::mem;

#[no_mangle]
pub extern "C" fn allocate(size: usize) -> *mut c_void {
    let mut buffer = Vec::with_capacity(size);
    let pointer = buffer.as_mut_ptr();
    // Say to compiler to forget about this memory cell
    // Deallocation will be done by who's going to consume this allocation
    mem::forget(buffer);

    pointer as *mut c_void
}