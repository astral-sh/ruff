use std::fmt::Formatter;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// Signals a [`CancellationToken`] that it should be canceled.
#[derive(Debug, Clone)]
pub struct CancellationTokenSource {
    cancelled: Arc<AtomicBool>,
}

impl Default for CancellationTokenSource {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationTokenSource {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_cancellation_requested(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Creates a new token that uses this source.
    pub fn token(&self) -> CancellationToken {
        CancellationToken {
            cancelled: self.cancelled.clone(),
        }
    }

    /// Requests cancellation for operations using this token.
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Token signals whether an operation should be canceled.
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// The operation was canceled by the provided [`CancellationToken`].
#[derive(Debug)]
pub struct Canceled;

impl std::error::Error for Canceled {}

impl std::fmt::Display for Canceled {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("operation was canceled")
    }
}
