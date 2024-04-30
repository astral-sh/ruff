use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct CancellationTokenSource {
    signal: Arc<AtomicBool>,
}

impl CancellationTokenSource {
    pub fn new() -> Self {
        Self {
            signal: Arc::new(AtomicBool::new(false)),
        }
    }

    #[tracing::instrument(level = "trace", skip_all)]
    pub fn cancel(&self) {
        self.signal.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.signal.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn token(&self) -> CancellationToken {
        CancellationToken {
            signal: self.signal.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CancellationToken {
    signal: Arc<AtomicBool>,
}

impl CancellationToken {
    /// Returns `true` if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.signal.load(std::sync::atomic::Ordering::SeqCst)
    }
}
