use std::num::NonZeroUsize;

use crate::sync::atomic::{AtomicUsize, Ordering};

/// Value of the initial revision, as a u64. We don't use 0
/// because we want to use a `NonZeroUsize`.
const START: usize = 1;

/// A unique identifier for the current version of the database.
///
/// Each time an input is changed, the revision number is incremented.
/// `Revision` is used internally to track which values may need to be
/// recomputed, but is not something you should have to interact with
/// directly as a user of salsa.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Revision {
    generation: NonZeroUsize,
}

impl Revision {
    #[inline]
    pub const fn max() -> Self {
        Self::from(usize::MAX)
    }

    #[inline]
    pub(crate) const fn start() -> Self {
        Self {
            // SAFETY: `START` is non-zero.
            generation: unsafe { NonZeroUsize::new_unchecked(START) },
        }
    }

    #[inline]
    pub(crate) const fn from(g: usize) -> Self {
        Self {
            generation: NonZeroUsize::new(g).unwrap(),
        }
    }

    #[inline]
    pub(crate) fn from_opt(g: usize) -> Option<Self> {
        NonZeroUsize::new(g).map(|generation| Self { generation })
    }

    #[inline]
    pub(crate) fn next(self) -> Revision {
        Self::from(self.generation.get() + 1)
    }

    #[inline]
    pub(crate) const fn as_usize(self) -> usize {
        self.generation.get()
    }
}

#[cfg(feature = "persistence")]
impl serde::Serialize for Revision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&self.as_usize(), serializer)
    }
}

#[cfg(feature = "persistence")]
impl<'de> serde::Deserialize<'de> for Revision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde::Deserialize::deserialize(deserializer).map(|generation| Self { generation })
    }
}

impl std::fmt::Debug for Revision {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(fmt, "R{}", self.generation)
    }
}

#[derive(Debug)]
pub struct AtomicRevision {
    data: AtomicUsize,
}

#[cfg(feature = "persistence")]
impl serde::Serialize for AtomicRevision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&self.data.load(Ordering::Relaxed), serializer)
    }
}

#[cfg(feature = "persistence")]
impl<'de> serde::Deserialize<'de> for AtomicRevision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let revision: Revision = serde::Deserialize::deserialize(deserializer)?;
        Ok(Self::new(revision))
    }
}

impl From<Revision> for AtomicRevision {
    fn from(value: Revision) -> Self {
        Self {
            data: AtomicUsize::new(value.as_usize()),
        }
    }
}

impl AtomicRevision {
    pub const fn start() -> Self {
        Self {
            data: AtomicUsize::new(START),
        }
    }

    pub const fn new(revision: Revision) -> Self {
        Self {
            data: AtomicUsize::new(revision.as_usize()),
        }
    }

    #[inline]
    pub fn load(&self) -> Revision {
        Revision {
            // SAFETY: We know that the value is non-zero because we only ever store `START` which 1, or a
            // Revision which is guaranteed to be non-zero.
            generation: unsafe { NonZeroUsize::new_unchecked(self.data.load(Ordering::Acquire)) },
        }
    }

    pub fn store(&self, r: Revision) {
        self.data.store(r.as_usize(), Ordering::Release);
    }

    pub fn get_mut(&mut self) -> &mut Revision {
        // SAFETY: `Revision` is repr(transparent) over `NonZeroUsize`, we only ever store `NonZeroUsize` values, thus making this cast valid.
        unsafe { &mut *(self.data.get_mut() as *mut usize).cast::<Revision>() }
    }
}

impl Clone for AtomicRevision {
    fn clone(&self) -> Self {
        Self {
            data: AtomicUsize::new(self.load().as_usize()),
        }
    }
}

#[derive(Debug)]
pub(crate) struct OptionalAtomicRevision {
    data: AtomicUsize,
}

#[cfg(feature = "persistence")]
impl serde::Serialize for OptionalAtomicRevision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&self.data.load(Ordering::Relaxed), serializer)
    }
}

#[cfg(feature = "persistence")]
impl<'de> serde::Deserialize<'de> for OptionalAtomicRevision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde::Deserialize::deserialize(deserializer).map(|data| Self {
            data: AtomicUsize::new(data),
        })
    }
}

impl From<Revision> for OptionalAtomicRevision {
    fn from(value: Revision) -> Self {
        Self {
            data: AtomicUsize::new(value.as_usize()),
        }
    }
}

impl OptionalAtomicRevision {
    pub(crate) fn new(revision: Option<Revision>) -> Self {
        Self {
            data: AtomicUsize::new(revision.map_or(0, |r| r.as_usize())),
        }
    }

    #[inline]
    pub(crate) fn load(&self) -> Option<Revision> {
        Revision::from_opt(self.data.load(Ordering::Acquire))
    }

    pub(crate) fn swap(&self, val: Option<Revision>) -> Option<Revision> {
        Revision::from_opt(
            self.data
                .swap(val.map_or(0, |r| r.as_usize()), Ordering::AcqRel),
        )
    }

    pub(crate) fn compare_exchange(
        &self,
        current: Option<Revision>,
        new: Option<Revision>,
    ) -> Result<Option<Revision>, Option<Revision>> {
        self.data
            .compare_exchange(
                current.map_or(0, |r| r.as_usize()),
                new.map_or(0, |r| r.as_usize()),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(Revision::from_opt)
            .map_err(Revision::from_opt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "persistence")]
    #[test]
    fn atomic_revision_rejects_zero_when_deserializing() {
        assert!(serde_json::from_str::<AtomicRevision>("0").is_err());
    }

    #[test]
    fn optional_atomic_revision() {
        let val = OptionalAtomicRevision::new(Some(Revision::start()));
        assert_eq!(val.load(), Some(Revision::start()));
        assert_eq!(val.swap(None), Some(Revision::start()));
        assert_eq!(val.load(), None);
        assert_eq!(val.swap(Some(Revision::start())), None);
        assert_eq!(val.load(), Some(Revision::start()));
        assert_eq!(
            val.compare_exchange(Some(Revision::start()), None),
            Ok(Some(Revision::start()))
        );
        assert_eq!(
            val.compare_exchange(Some(Revision::start()), None),
            Err(None)
        );
    }
}
