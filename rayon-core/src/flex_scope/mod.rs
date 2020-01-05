use job::HeapJob;
use latch::LatchProbe;
use registry::{in_worker, Registry};
use std::any::Any;
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::Mutex;
use tlv;
use unwind;

struct ActiveFlexScope {
    /// thread registry where `scope()` was executed.
    registry: Arc<Registry>,

    /// if some job panicked, the error is stored here; it will be
    /// propagated to the one who created the scope
    panic: Option<Box<dyn Any + Send + 'static>>,

    active_jobs: usize,

    terminated: bool,

    /// The TLV at the scope's creation. Used to set the TLV for spawned jobs.
    tlv: usize,
}

pub struct FlexScope<'scope> {
    data: Mutex<Option<ActiveFlexScope>>,
    marker: PhantomData<fn(&'scope ()) -> &'scope ()>,
}

impl<'scope> FlexScope<'scope> {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(None),
            marker: PhantomData,
        }
    }

    pub fn activate<R>(&self, f: impl FnOnce() -> R) -> R {
        // Activate the scope
        let tlv = tlv::get();
        {
            let mut data = self.data.lock().unwrap();
            assert!(data.is_none(), "scope was already activated");
            *data = Some(ActiveFlexScope {
                active_jobs: 1,
                registry: Registry::current(),
                panic: None,
                terminated: false,
                tlv,
            })
        }

        // Run the closure
        let result = self.execute_job(f);

        // Wait on the remaining tasks.
        in_worker(|owner_thread, _| unsafe {
            owner_thread.wait_until(&self);
        });

        // Restore the TLV if we ran some jobs while waiting
        tlv::set(tlv);

        let panic = {
            let mut data = self.data.lock().unwrap();
            let panic = data.as_mut().unwrap().panic.take();

            // Deactivate the scope
            *data = None;

            panic
        };

        if let Some(panic) = panic {
            unwind::resume_unwinding(panic);
        }

        result.unwrap()
    }

    pub fn spawn(&self, f: impl FnOnce() + Send + 'scope) {
        let mut data = self.data.lock().unwrap();
        let data = data.as_mut().expect("the scope is not active");
        assert!(!data.terminated, "the scope is terminated");
        assert!(data.active_jobs != std::usize::MAX);
        data.active_jobs += 1;

        let job_ref = unsafe {
            Box::new(HeapJob::new(data.tlv, move || {
                self.execute_job(move || f());
            }))
            .as_job_ref()
        };

        // Since `Scope` implements `Sync`, we can't be sure that we're still in a
        // thread of this pool, so we can't just push to the local worker thread.
        data.registry.inject_or_push(job_ref);
    }

    fn execute_job<R>(&self, f: impl FnOnce() -> R) -> Option<R> {
        let result = unwind::halt_unwinding(f);
        let mut data = self.data.lock().unwrap();
        let data = data.as_mut().unwrap();
        data.active_jobs -= 1;
        if data.active_jobs == 0 {
            // Mark the scope as terminated once the job count hits 0.
            // This ensures other threads cannot spawn more jobs.
            data.terminated = true;
        }
        result.map(|r| Some(r)).unwrap_or_else(|panic| {
            data.panic = Some(panic);
            None
        })
    }
}

impl<'scope> LatchProbe for FlexScope<'scope> {
    fn probe(&self) -> bool {
        self.data.lock().unwrap().as_ref().unwrap().active_jobs == 0
    }
}
