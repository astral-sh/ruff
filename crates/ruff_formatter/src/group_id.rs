use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
pub struct DebugGroupId {
    value: NonZeroU32,
    #[cfg_attr(feature = "serde", serde(skip))]
    name: &'static str,
}

impl DebugGroupId {
    #[allow(unused)]
    fn new(value: NonZeroU32, debug_name: &'static str) -> Self {
        Self {
            value,
            name: debug_name,
        }
    }
}

impl std::fmt::Debug for DebugGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}-{}", self.name, self.value)
    }
}

/// Unique identification for a group.
///
/// See [`crate::Formatter::group_id`] on how to get a unique id.
#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ReleaseGroupId {
    value: NonZeroU32,
}

impl ReleaseGroupId {
    /// Creates a new unique group id with the given debug name (only stored in debug builds)
    #[allow(unused)]
    fn new(value: NonZeroU32, _: &'static str) -> Self {
        Self { value }
    }
}

impl From<GroupId> for u32 {
    fn from(id: GroupId) -> Self {
        id.value.get()
    }
}

impl std::fmt::Debug for ReleaseGroupId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{}", self.value)
    }
}

#[cfg(not(debug_assertions))]
pub type GroupId = ReleaseGroupId;
#[cfg(debug_assertions)]
pub type GroupId = DebugGroupId;

/// Builder to construct unique group ids that are unique if created with the same builder.
pub(super) struct UniqueGroupIdBuilder {
    next_id: AtomicU32,
}

impl UniqueGroupIdBuilder {
    /// Creates a new unique group id with the given debug name.
    pub(crate) fn group_id(&self, debug_name: &'static str) -> GroupId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let id = NonZeroU32::new(id).unwrap_or_else(|| panic!("Group ID counter overflowed"));

        GroupId::new(id, debug_name)
    }
}

impl Default for UniqueGroupIdBuilder {
    fn default() -> Self {
        UniqueGroupIdBuilder {
            // Start with 1 because `GroupId` wraps a `NonZeroU32` to reduce memory usage.
            next_id: AtomicU32::new(1),
        }
    }
}
