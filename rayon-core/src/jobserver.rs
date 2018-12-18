use jobserver_crate::{Client, HelperThread, Acquired};
use std::sync::{Condvar, Arc, Mutex, Weak};
use std::mem;

#[derive(Default)]
pub struct LockedProxyData {
    /// The number of free thread tokens, this may include the implicit token given to the process
    free: usize,

    /// The number of threads waiting for a token
    waiters: usize,

    /// The number of tokens we requested from the server
    requested: usize,

    /// Stored tokens which will be dropped when we no longer need them
    tokens: Vec<Acquired>,
}

impl LockedProxyData {
    fn request_token(&mut self, thread: &Mutex<HelperThread>) {
        self.requested += 1;
        thread.lock().unwrap().request_token();
    }

    fn return_token(&mut self, cond_var: &Condvar) {
        if self.waiters > 0 {
            self.free += 1;
            cond_var.notify_one();
        } else {
            if self.tokens.is_empty() {
                // We are returning the implicit token
                self.free += 1;
            } else {
                // Return a real token to the server
                self.tokens.pop().unwrap();
            }
        }
    }

    fn take_token(&mut self, thread: &Mutex<HelperThread>) -> bool {
        if self.free > 0 {
            self.free -= 1;
            self.waiters -= 1;

            // We stole some token reqested by someone else
            // Request another one
            if self.requested + self.free < self.waiters {
                self.request_token(thread);
            }

            true
        } else {
            false
        }
    }

    fn new_requested_token(&mut self, token: Acquired, cond_var: &Condvar) {
        self.requested -= 1;

        // Does anything need this token?
        if self.waiters > 0 {
            self.free += 1;
            self.tokens.push(token);
            cond_var.notify_one();
        } else {
            // Otherwise we'll just drop it
            mem::drop(token);
        }
    }
}

#[derive(Default)]
pub struct ProxyData {
    lock: Mutex<LockedProxyData>,
    cond_var: Condvar,
}

pub struct Proxy {
    thread: Option<Mutex<HelperThread>>,
    data: Arc<ProxyData>,
}

lazy_static! {
    // We can only call `from_env` once per process
    static ref GLOBAL_CLIENT: Option<Client> = unsafe { Client::from_env() };

    // We only want one Proxy to exists at a time
    static ref CURRENT_PROXY: Mutex<Option<Weak<Proxy>>> = Mutex::new(None);
}

impl Proxy {
    pub fn from_env() -> Arc<Self> {
        let mut current = CURRENT_PROXY.lock().unwrap();
        if let Some(current) = current.as_ref().and_then(|c| c.upgrade()) {
            // There is a proxy around already
            current
        } else {
            // Create a new proxy
            let data = Arc::new(ProxyData::default());
            let data2 = data.clone();

            let proxy = Arc::new(Proxy {
                thread: GLOBAL_CLIENT.clone().map(|c| {
                    Mutex::new(c.into_helper_thread(move |token| {
                        data2.lock.lock().unwrap().new_requested_token(token.unwrap(), &data2.cond_var);
                    }).unwrap())
                }),
                data,
            });
            *current = Some(Arc::downgrade(&proxy));
            proxy
        }
    }

    pub fn disabled() -> Arc<Self> {
        Arc::new(Proxy {
            thread: None,
            data: Arc::new(ProxyData::default()),
        })
    }

    pub fn return_token(&self) {
        if self.thread.is_some() {
            self.data.lock.lock().unwrap().return_token(&self.data.cond_var);
        }
    }

    pub fn acquire_token(&self) {
        if let Some(ref thread) = self.thread {
            let mut data = self.data.lock.lock().unwrap();
            data.waiters += 1;
            if data.take_token(thread) {
                return;
            }
            // Request a token for us
            data.request_token(thread);
            loop {
                data = self.data.cond_var.wait(data).unwrap();
                if data.take_token(thread) {
                    return;
                }
            }
        }
    }
}
