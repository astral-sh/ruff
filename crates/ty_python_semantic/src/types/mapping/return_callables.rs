//! Immutable replacements used to move function type variables into returned callables.

use std::fmt;

use crate::FxIndexMap;
use rustc_hash::FxHashMap;

use crate::types::{BoundTypeVarInstance, CallableType};

/// Maps original variables to renamed declarations for a returned callable.
/// Source resource storage admits and retains this completed map before mapping children use it.
#[derive(Debug)]
pub(in crate::types) struct ReturnTypevarMap<'db> {
    pub(in crate::types) values: FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>,
}

/// Maps original callable handles to the callables with renamed generic declarations.
/// Source resource storage admits and retains this completed map until mapping children drain.
#[derive(Debug)]
pub(in crate::types) struct ReturnCallableMap<'db> {
    pub(in crate::types) values: FxHashMap<CallableType<'db>, CallableType<'db>>,
}

/// A stable view of original-to-renamed variables, compared and debugged by owner identity.
#[derive(Clone, Copy)]
pub struct RetainedReturnTypevars<'owner, 'db> {
    owner: &'owner ReturnTypevarMap<'db>,
}

impl<'owner, 'db> RetainedReturnTypevars<'owner, 'db> {
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) const fn new(owner: &'owner ReturnTypevarMap<'db>) -> Self {
        Self { owner }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn len(self) -> usize {
        self.owner.values.len()
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn capacity(self) -> usize {
        self.owner.values.capacity()
    }

    pub(in crate::types) fn get(
        self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Option<BoundTypeVarInstance<'db>> {
        self.owner.values.get(&variable).copied()
    }

    /// Reads a renamed variable in the original map's insertion order.
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn value_at(self, index: usize) -> Option<BoundTypeVarInstance<'db>> {
        self.owner.values.get_index(index).map(|(_, value)| *value)
    }

    pub(in crate::types) fn same_storage(self, other: RetainedReturnTypevars<'_, 'db>) -> bool {
        std::ptr::eq(self.owner, other.owner)
    }

    #[cfg(test)]
    pub(in crate::types) fn owner_identity(self) -> usize {
        std::ptr::from_ref(self.owner).addr()
    }
}

impl fmt::Debug for RetainedReturnTypevars<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetainedReturnTypevars")
            .field("owner", &std::ptr::from_ref(self.owner))
            .finish()
    }
}

impl PartialEq for RetainedReturnTypevars<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        self.same_storage(*other)
    }
}

impl Eq for RetainedReturnTypevars<'_, '_> {}
impl get_size2::GetSize for RetainedReturnTypevars<'_, '_> {}

/// A stable view of callable-handle replacements; it never owns or borrows a mutable map.
#[derive(Clone, Copy)]
pub struct RetainedReturnCallables<'owner, 'db> {
    owner: &'owner ReturnCallableMap<'db>,
}

impl<'owner, 'db> RetainedReturnCallables<'owner, 'db> {
    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) const fn new(owner: &'owner ReturnCallableMap<'db>) -> Self {
        Self { owner }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn len(self) -> usize {
        self.owner.values.len()
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn capacity(self) -> usize {
        self.owner.values.capacity()
    }

    pub(in crate::types) fn get(self, callable: CallableType<'db>) -> Option<CallableType<'db>> {
        self.owner.values.get(&callable).copied()
    }

    pub(in crate::types) fn same_storage(self, other: RetainedReturnCallables<'_, 'db>) -> bool {
        std::ptr::eq(self.owner, other.owner)
    }

    #[cfg(test)]
    pub(in crate::types) fn owner_identity(self) -> usize {
        std::ptr::from_ref(self.owner).addr()
    }
}

impl fmt::Debug for RetainedReturnCallables<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RetainedReturnCallables")
            .field("owner", &std::ptr::from_ref(self.owner))
            .finish()
    }
}

impl PartialEq for RetainedReturnCallables<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        self.same_storage(*other)
    }
}

impl Eq for RetainedReturnCallables<'_, '_> {}
impl get_size2::GetSize for RetainedReturnCallables<'_, '_> {}

/// Original-to-renamed variables borrowed by ordinary mapping or retained by controlled mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize)]
pub enum ReturnTypevarReplacements<'a, 'db> {
    Borrowed(&'a FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>),
    Retained(RetainedReturnTypevars<'a, 'db>),
}

impl<'db> ReturnTypevarReplacements<'_, 'db> {
    pub(in crate::types) fn get(
        self,
        variable: BoundTypeVarInstance<'db>,
    ) -> Option<BoundTypeVarInstance<'db>> {
        match self {
            Self::Borrowed(values) => values.get(&variable).copied(),
            Self::Retained(values) => values.get(variable),
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn len(self) -> usize {
        match self {
            Self::Borrowed(values) => values.len(),
            Self::Retained(values) => values.len(),
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn capacity(self) -> usize {
        match self {
            Self::Borrowed(values) => values.capacity(),
            Self::Retained(values) => values.capacity(),
        }
    }
}

/// Callable replacements borrowed by ordinary mapping or retained by controlled mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize)]
pub enum ReturnCallableReplacements<'a, 'db> {
    Borrowed(&'a FxHashMap<CallableType<'db>, CallableType<'db>>),
    Retained(RetainedReturnCallables<'a, 'db>),
}

impl<'db> ReturnCallableReplacements<'_, 'db> {
    pub(in crate::types) fn get(self, callable: CallableType<'db>) -> Option<CallableType<'db>> {
        match self {
            Self::Borrowed(values) => values.get(&callable).copied(),
            Self::Retained(values) => values.get(callable),
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn len(self) -> usize {
        match self {
            Self::Borrowed(values) => values.len(),
            Self::Retained(values) => values.len(),
        }
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn capacity(self) -> usize {
        match self {
            Self::Borrowed(values) => values.capacity(),
            Self::Retained(values) => values.capacity(),
        }
    }
}
