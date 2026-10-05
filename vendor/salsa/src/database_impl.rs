use tracing::Level;

use crate::storage::HasStorage;
use crate::{Database, Storage};

/// Default database implementation that you can use if you don't
/// require any custom user data.
#[derive(Clone)]
pub struct DatabaseImpl {
    storage: Storage<Self>,
}

impl Default for DatabaseImpl {
    fn default() -> Self {
        Self {
            // Default behavior: trace events at DEBUG when detailed tracing is enabled.
            storage: Storage::new(
                if cfg!(feature = "detailed-trace") && tracing::enabled!(Level::DEBUG) {
                    Some(Box::new(|event| {
                        crate::tracing::debug!("salsa_event({:?})", event)
                    }))
                } else {
                    None
                },
            ),
        }
    }
}

impl DatabaseImpl {
    /// Create a new database; equivalent to `Self::default`.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn storage(&self) -> &Storage<Self> {
        &self.storage
    }
}

impl Database for DatabaseImpl {}

// SAFETY: The `storage` and `storage_mut` fields return a reference to the same storage field owned by `self`.
unsafe impl HasStorage for DatabaseImpl {
    #[inline(always)]
    fn storage(&self) -> &Storage<Self> {
        &self.storage
    }

    #[inline(always)]
    fn storage_mut(&mut self) -> &mut Storage<Self> {
        &mut self.storage
    }
}
