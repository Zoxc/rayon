//! Allows access to the Rayon's thread local value
//! which is preserved when moving jobs across threads

use std::cell::Cell;

thread_local!(pub(crate) static TLV: Cell<usize> = Cell::new(0));

/// Gives access to the thread-local value inside the closure.
pub fn with<F: FnOnce(&Cell<usize>) -> R, R>(f: F) -> R {
    TLV.with(f)
}

/// Sets the current thread-local value
pub(crate) fn set(value: usize) {
    TLV.with(|tlv| tlv.set(value));
}

/// Returns the current thread-local value
pub(crate) fn get() -> usize {
    TLV.with(|tlv| tlv.get())
}
