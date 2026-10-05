#![allow(unused)]

use super::sync::{Condvar, Mutex};

#[derive(Default)]
pub(crate) struct Signal {
    value: Mutex<usize>,
    cond_var: Condvar,
}

impl Signal {
    pub(crate) fn signal(&self, stage: usize) {
        // When running with shuttle we want to explore as many possible
        // executions, so we avoid signals entirely.
        #[cfg(not(feature = "shuttle"))]
        {
            // This check avoids acquiring the lock for things that will
            // clearly be a no-op. Not *necessary* but helps to ensure we
            // are more likely to encounter weird race conditions;
            // otherwise calls to `sum` will tend to be unnecessarily
            // synchronous.
            if stage > 0 {
                let mut v = self.value.lock().unwrap();
                if stage > *v {
                    *v = stage;
                    self.cond_var.notify_all();
                }
            }
        }
    }

    /// Waits until the given condition is true; the fn is invoked
    /// with the current stage.
    pub(crate) fn wait_for(&self, stage: usize) {
        #[cfg(not(feature = "shuttle"))]
        {
            // As above, avoid lock if clearly a no-op.
            if stage > 0 {
                let mut v = self.value.lock().unwrap();
                while *v < stage {
                    v = self.cond_var.wait(v).unwrap();
                }
            }
        }
    }
}
