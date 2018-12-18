//! Allows access to the Rayon's thread local value
//! which is preserved when moving jobs across threads

use std::cell::Cell;

#[thread_local]
static TLV: Cell<usize> = Cell::new(0);

/// Sets the current thread-local value to `value` inside the closure.
/// The old value is restored when the closure ends
pub fn with<F: FnOnce() -> R, R>(value: usize, f: F) -> R {
    struct Reset(usize);
    impl Drop for Reset {
        fn drop(&mut self) {
            set(self.0);
        }
    }
    let _reset = Reset(get());
    set(value);
    f()
}

/// Sets the current thread-local value
#[inline(never)] // Must have inline(never) so the use of TLV doesn't escape the crate
pub fn set(value: usize) {
    TLV.set(value);
}

/// Returns the current thread-local value
#[inline(never)] // Must have inline(never) so the use of TLV doesn't escape the crate
pub fn get() -> usize {
    TLV.get()
}
