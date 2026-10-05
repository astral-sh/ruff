use crate::Id;
use crate::sync::atomic::{AtomicU64, Ordering};

mod sealed {
    pub trait Sealed {}
}

pub trait SingletonChoice: sealed::Sealed + Default {
    fn with_scope(&self, cb: impl FnOnce() -> Id) -> Id;
    fn index(&self) -> Option<Id>;
}

pub struct Singleton {
    index: AtomicU64,
}
impl sealed::Sealed for Singleton {}
impl SingletonChoice for Singleton {
    fn with_scope(&self, cb: impl FnOnce() -> Id) -> Id {
        if self.index.load(Ordering::Acquire) != 0 {
            panic!("singleton struct may not be duplicated");
        }
        let id = cb();
        if self
            .index
            .compare_exchange(0, id.as_bits(), Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            panic!("singleton struct may not be duplicated");
        }
        id
    }

    fn index(&self) -> Option<Id> {
        match self.index.load(Ordering::Acquire) {
            0 => None,

            // SAFETY: Our u64 is derived from an ID and thus safe to convert back.
            id => Some(unsafe { Id::from_bits_unchecked(id) }),
        }
    }
}

impl Default for Singleton {
    fn default() -> Self {
        Self {
            index: AtomicU64::new(0),
        }
    }
}
#[derive(Default)]
pub struct NotSingleton;
impl sealed::Sealed for NotSingleton {}
impl SingletonChoice for NotSingleton {
    fn with_scope(&self, cb: impl FnOnce() -> Id) -> Id {
        cb()
    }
    fn index(&self) -> Option<Id> {
        None
    }
}
