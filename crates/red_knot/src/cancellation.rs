use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug, Default)]
pub struct CancellationSource {
    signal: Arc<(Mutex<bool>, Condvar)>,
}

impl CancellationSource {
    pub fn new() -> Self {
        Self {
            signal: Arc::new((Mutex::new(false), Condvar::default())),
        }
    }

    pub fn cancel(&self) {
        let (cancelled, condvar) = &*self.signal;

        let mut cancelled = cancelled.lock().unwrap();

        if *cancelled {
            return;
        }

        *cancelled = true;
        condvar.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        let (cancelled, _) = &*self.signal;

        *cancelled.lock().unwrap()
    }

    pub fn token(&self) -> CancellationToken {
        CancellationToken {
            signal: self.signal.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CancellationToken {
    signal: Arc<(Mutex<bool>, Condvar)>,
}

impl CancellationToken {
    /// Returns `true` if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        let (cancelled, _) = &*self.signal;

        *cancelled.lock().unwrap()
    }

    pub fn wait(&self) {
        let (bool, condvar) = &*self.signal;

        let lock = condvar
            .wait_while(bool.lock().unwrap(), |bool| !*bool)
            .unwrap();

        debug_assert!(*lock);

        drop(lock);
    }
}
