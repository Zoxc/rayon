//! Allows access to the Rayon's thread local value
//! which is preserved when moving jobs across threads

use std::cell::Cell;

thread_local!(pub static TLV: Cell<*const ()> = Cell::new(std::ptr::null()));

/// Sets the current thread-local value
pub fn set(value: usize) {
    TLV.with(|tlv| tlv.set(value as *const ()));
}

/// Returns the current thread-local value
pub fn get() -> usize {
    TLV.with(|tlv| tlv.get()as *const ()) as usize
}
