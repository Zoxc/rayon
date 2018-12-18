use registry::{Registry, WorkerThread};
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

#[repr(align(64))]
#[derive(Debug)]
struct CacheAligned<T>(T);

/// Holds worker-locals values for each thread in a thread pool.
/// You can only access the worker local value through the Deref impl
/// on the thread pool it was constructed on. It will panic otherwise
pub struct WorkerLocal<T> {
    locals: Vec<CacheAligned<T>>,
    registry: Arc<Registry>,
}

unsafe impl<T> Send for WorkerLocal<T> {}
unsafe impl<T> Sync for WorkerLocal<T> {}

#[inline(never)]
#[cold]
fn wrong_thread() -> ! {
    panic!("WorkerLocal can only be used on the thread pool it was created on")
}

// Must have inline(never) so the use of WORKER_THREAD_STATE doesn't escape the crate
#[inline(never)]
fn thread_check(reg: &Registry) -> usize {
    unsafe {
        let worker_thread = WorkerThread::unsafe_current();
        let wrong = worker_thread.is_null()
            || &*(*worker_thread).registry as *const _ != reg as *const _;
        if wrong {
            wrong_thread();
        }
        (*worker_thread).index
    }
}

impl<T> WorkerLocal<T> {
    /// Creates a new worker local where the `initial` closure computes the
    /// value this worker local should take for each thread in the thread pool.
    #[inline]
    pub fn new<F: FnMut(usize) -> T>(mut initial: F) -> WorkerLocal<T> {
        let registry = Registry::current();
        WorkerLocal {
            locals: (0..registry.num_threads())
                .map(|i| CacheAligned(initial(i)))
                .collect(),
            registry,
        }
    }

    /// Returns the worker-local value for each thread
    #[inline]
    pub fn into_inner(self) -> Vec<T> {
        self.locals.into_iter().map(|c| c.0).collect()
    }

    fn current(&self) -> &T {
        unsafe {
            let idx = thread_check(&self.registry);
            &self.locals.get_unchecked(idx).0
        }
    }
}

impl<T> WorkerLocal<Vec<T>> {
    /// Joins the elements of all the worker locals into one Vec
    pub fn join(self) -> Vec<T> {
        self.into_inner().into_iter().flat_map(|v| v).collect()
    }
}

impl<T: fmt::Debug> fmt::Debug for WorkerLocal<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(&self.locals, f)
    }
}

impl<T> Deref for WorkerLocal<T> {
    type Target = T;

    #[inline(always)]
    fn deref(&self) -> &T {
        self.current()
    }
}