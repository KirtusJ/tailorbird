//! Byte comparisons required by the stable Linux target's prebuilt core in a
//! freestanding worker. The shared guest ABI already supplies copy and fill.

/// Both inputs must be readable for `len` bytes. A zero length reads neither.
#[no_mangle]
pub unsafe extern "C" fn memcmp(left: *const u8, right: *const u8, len: usize) -> i32 {
    for offset in 0..len {
        // Volatile reads keep LLVM from folding this definition into a recursive
        // call to the very libc routine that it implements.
        let a = unsafe { left.add(offset).read_volatile() };
        let b = unsafe { right.add(offset).read_volatile() };
        if a != b {
            return i32::from(a) - i32::from(b);
        }
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn bcmp(left: *const u8, right: *const u8, len: usize) -> i32 {
    unsafe { memcmp(left, right, len) }
}
