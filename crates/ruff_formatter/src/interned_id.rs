use std::sync::atomic::{AtomicU32, Ordering};

#[allow(unreachable_pub)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct DebugInternedId {
    value: u32,
    #[cfg_attr(feature = "serde", serde(skip))]
    name: &'static str,
}

impl DebugInternedId {
    #[allow(unused)]
    fn new(value: u32, debug_name: &'static str) -> Self {
        Self {
            value,
            name: debug_name,
        }
    }
}

impl std::fmt::Debug for DebugInternedId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}-{}", self.name, self.value)
    }
}

/// ID uniquely identifying a range in a document.
///
/// See [`crate::Formatter::intern`] on how to intern content.
#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(unreachable_pub)]
pub struct ReleaseInternedId {
    value: u32,
}

impl ReleaseInternedId {
    #[allow(unused)]
    fn new(value: u32, _: &'static str) -> Self {
        Self { value }
    }
}

impl std::fmt::Debug for ReleaseInternedId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.value)
    }
}

#[cfg(not(debug_assertions))]
#[allow(unreachable_pub)]
pub type InternedId = ReleaseInternedId;
#[cfg(debug_assertions)]
#[allow(unreachable_pub)]
pub type InternedId = DebugInternedId;

impl From<InternedId> for u32 {
    fn from(id: InternedId) -> Self {
        id.value
    }
}

/// Builder to construct [`InternedId`]s that are unique if created with the same builder.
pub(super) struct UniqueInternedIdBuilder {
    next_id: AtomicU32,
}

impl UniqueInternedIdBuilder {
    /// Creates a new unique [`InternedId`] with the given debug name.
    pub(crate) fn new_id(&self, debug_name: &'static str) -> InternedId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        if id == u32::MAX {
            panic!("Interned ID counter overflowed");
        }

        InternedId::new(id, debug_name)
    }
}

impl Default for UniqueInternedIdBuilder {
    fn default() -> Self {
        UniqueInternedIdBuilder {
            next_id: AtomicU32::new(0),
        }
    }
}

/// Map indexed by [`InternedId`]. Uses a [`Vec`] internally, making use of the fact that
/// [`InternedId`]s are monotonic.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct InternedIndex<T>(Vec<Option<T>>);

impl<T> InternedIndex<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn get(&self, id: InternedId) -> Option<&T> {
        let index = u32::from(id) as usize;

        match self.0.get(index) {
            Some(Some(value)) => Some(value),
            Some(None) | None => None,
        }
    }

    pub fn insert(&mut self, id: InternedId, value: T) {
        let index = u32::from(id) as usize;

        if self.0.len() <= index {
            self.0.resize_with(index + 1, || None);
        }

        self.0[index] = Some(value);
    }
}
