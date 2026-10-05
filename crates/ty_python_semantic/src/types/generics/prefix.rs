//! Partial substitutions borrow ordinary arguments or an immutable initialized prefix.

use std::fmt;
use std::sync::OnceLock;

use crate::types::Type;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct PrefixReadError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) enum PrefixWriteError {
    Full,
    Initialized,
}

/// Slots never move or change after initialization. The owning resource storage outlives all
/// mapping tasks that retain a prefix, including tasks interrupted before their parent resumes.
pub(in crate::types) struct DefaultArgumentSlots<'db> {
    slots: Box<[OnceLock<Type<'db>>]>,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'db> DefaultArgumentSlots<'db> {
    /// The caller admits initialization, backing storage and eventual disposal before construction.
    pub(in crate::types) fn new(len: usize) -> Self {
        let mut slots = Vec::with_capacity(len);
        slots.resize_with(len, OnceLock::new);
        Self {
            slots: slots.into_boxed_slice(),
        }
    }

    pub(in crate::types) fn buffer(&self) -> DefaultArgumentBuffer<'_, 'db> {
        DefaultArgumentBuffer {
            owner: self,
            initialized: 0,
        }
    }

    pub(in crate::types) fn capacity(&self) -> usize {
        self.slots.len()
    }
}

/// Only the filling algorithm owns the cursor. Prefix consumers cannot initialize a slot or
/// observe a later argument through a previously captured view.
#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) struct DefaultArgumentBuffer<'owner, 'db> {
    owner: &'owner DefaultArgumentSlots<'db>,
    initialized: usize,
}

#[cfg(any(test, feature = "experimental-analysis"))]
impl<'owner, 'db> DefaultArgumentBuffer<'owner, 'db> {
    pub(in crate::types) fn len(&self) -> usize {
        self.initialized
    }

    pub(in crate::types) fn capacity(&self) -> usize {
        self.owner.capacity()
    }

    pub(in crate::types) fn append(&mut self, ty: Type<'db>) -> Result<(), PrefixWriteError> {
        let Some(slot) = self.owner.slots.get(self.initialized) else {
            return Err(PrefixWriteError::Full);
        };
        slot.set(ty).map_err(|_| PrefixWriteError::Initialized)?;
        self.initialized += 1;
        Ok(())
    }

    pub(in crate::types) fn prefix(&self) -> InitializedTypePrefix<'owner, 'db> {
        InitializedTypePrefix {
            owner: self.owner,
            len: self.initialized,
        }
    }
}

/// Capturing or comparing this view never traverses the prefix contents.
#[derive(Clone, Copy)]
pub struct InitializedTypePrefix<'owner, 'db> {
    owner: &'owner DefaultArgumentSlots<'db>,
    len: usize,
}

impl<'owner, 'db> InitializedTypePrefix<'owner, 'db> {
    #[cfg(test)]
    pub(in crate::types) fn len(self) -> usize {
        self.len
    }

    pub(in crate::types) fn get(self, index: usize) -> Option<Type<'db>> {
        if index >= self.len {
            return None;
        }
        self.owner.slots.get(index).and_then(OnceLock::get).copied()
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn checked_get(
        self,
        index: usize,
    ) -> Result<Option<Type<'db>>, PrefixReadError> {
        if index >= self.len {
            return Ok(None);
        }
        self.get(index).map(Some).ok_or(PrefixReadError)
    }

    pub(in crate::types) fn same_storage(self, other: InitializedTypePrefix<'_, 'db>) -> bool {
        std::ptr::eq(self.owner, other.owner) && self.len == other.len
    }

    #[cfg(test)]
    pub(in crate::types) fn owner_identity(self) -> usize {
        std::ptr::from_ref(self.owner).addr()
    }
}

impl fmt::Debug for InitializedTypePrefix<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InitializedTypePrefix")
            .field("owner", &std::ptr::from_ref(self.owner))
            .field("len", &self.len)
            .finish()
    }
}

impl PartialEq for InitializedTypePrefix<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        self.same_storage(*other)
    }
}

impl Eq for InitializedTypePrefix<'_, '_> {}

impl get_size2::GetSize for InitializedTypePrefix<'_, '_> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize)]
pub enum TypeArgumentPrefix<'a, 'db> {
    Borrowed(&'a [Type<'db>]),
    Retained(InitializedTypePrefix<'a, 'db>),
}

impl<'a, 'db> TypeArgumentPrefix<'a, 'db> {
    pub(in crate::types) fn get(self, index: usize) -> Option<Type<'db>> {
        match self {
            Self::Borrowed(types) => types.get(index).copied(),
            Self::Retained(prefix) => prefix.get(index),
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn checked_get(
        self,
        index: usize,
    ) -> Result<Option<Type<'db>>, PrefixReadError> {
        match self {
            Self::Borrowed(types) => Ok(types.get(index).copied()),
            Self::Retained(prefix) => prefix.checked_get(index),
        }
    }
}

impl<'a, 'db> From<&'a [Type<'db>]> for TypeArgumentPrefix<'a, 'db> {
    fn from(types: &'a [Type<'db>]) -> Self {
        Self::Borrowed(types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialized_views_keep_their_length_and_owner() {
        let slots = DefaultArgumentSlots::new(3);
        let mut buffer = slots.buffer();
        let empty = buffer.prefix();
        assert_eq!(buffer.append(Type::any()), Ok(()));
        let first = buffer.prefix();
        assert_eq!(buffer.append(Type::unknown()), Ok(()));
        let second = buffer.prefix();
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.checked_get(0), Ok(None));
        assert_eq!(first.checked_get(0), Ok(Some(Type::any())));
        assert_eq!(first.checked_get(1), Ok(None));
        assert_eq!(second.checked_get(1), Ok(Some(Type::unknown())));
        assert!(std::ptr::eq(first.owner, second.owner));
        assert_eq!(buffer.capacity(), 3);
        assert_eq!(buffer.append(Type::Never), Ok(()));
        assert_eq!(buffer.append(Type::any()), Err(PrefixWriteError::Full));
    }

    #[test]
    fn argument_storage_is_one_fixed_array_for_all_prefixes() {
        let slots = DefaultArgumentSlots::new(256);
        let mut buffer = slots.buffer();
        let address = slots.slots.as_ptr();
        for index in 0..256 {
            let prefix = buffer.prefix();
            assert_eq!(prefix.len(), index);
            assert_eq!(prefix.owner.slots.as_ptr(), address);
            assert_eq!(prefix.owner.slots.len(), 256);
            assert_eq!(buffer.append(Type::any()), Ok(()));
        }
        assert_eq!(buffer.prefix().checked_get(255), Ok(Some(Type::any())));
    }

    #[test]
    fn a_claimed_initialized_hole_is_a_contract_error() {
        let owner = DefaultArgumentSlots::new(1);
        let invalid = InitializedTypePrefix {
            owner: &owner,
            len: 1,
        };
        assert_eq!(invalid.checked_get(0), Err(PrefixReadError));
    }

    #[test]
    fn prefix_transport_preserves_thread_safety() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<TypeArgumentPrefix<'static, 'static>>();
        send_sync::<crate::types::TypeMapping<'static, 'static>>();
    }
}
