//! Tracks the support of each constraint and interior node in a BDD.
//!
//! The support of a constraint is the set of typevars mentioned anywhere in the constraint
//! (either the subject, or anywhere in the lower or upper bound).
//!
//! The support of a node is the union of the supports of every constraint reachable from that
//! node.

use std::ops::BitOrAssign;

use crate::types::constraints::TypeVarId;
use crate::types::constraints::bitset::BitSet;

use ruff_index::newtype_index;

#[newtype_index]
#[derive(get_size2::GetSize)]
pub(super) struct SupportId;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct Support {
    bits: BitSet,
    has_skipped_lazy_attributes: bool,
}

impl Support {
    /// Adds a typevar to this support.
    pub(super) fn insert(&mut self, typevar: TypeVarId) {
        let index = typevar.index();
        self.bits.insert(index);
    }

    /// Returns an iterator of all of the typevars in this support.
    pub(super) fn iter(&self) -> impl Iterator<Item = TypeVarId> + '_ {
        self.bits.iter().map(TypeVarId::from_usize)
    }

    /// Returns whether this support contains any type variables in common with `other`.
    pub(super) fn overlaps_with(&self, other: &Self) -> bool {
        self.bits.overlaps_with(&other.bits)
    }

    /// Records that lazy type attributes may contain additional type variables.
    pub(super) fn mark_incomplete(&mut self) {
        self.has_skipped_lazy_attributes = true;
    }

    /// Returns whether all type attributes were inspected while collecting this support.
    pub(super) fn is_complete(&self) -> bool {
        !self.has_skipped_lazy_attributes
    }
}

impl BitOrAssign<&Self> for Support {
    fn bitor_assign(&mut self, rhs: &Self) {
        self.bits |= &rhs.bits;
        self.has_skipped_lazy_attributes |= rhs.has_skipped_lazy_attributes;
    }
}

impl BitOrAssign<Option<&Self>> for Support {
    fn bitor_assign(&mut self, rhs: Option<&Self>) {
        if let Some(rhs) = rhs {
            *self |= rhs;
        }
    }
}
