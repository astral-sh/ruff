//! Tracks the support of each constraint and interior node in a BDD.
//!
//! The support of a constraint is the set of typevars mentioned anywhere in the constraint
//! (either the subject, or anywhere in the lower or upper bound).
//!
//! The support of a node is the union of the supports of every constraint reachable from that
//! node.

use std::ops::BitOrAssign;

use crate::types::constraints::TypeVarId;

use bitvec::prelude::BitVec;
use ruff_index::newtype_index;

#[newtype_index]
#[derive(get_size2::GetSize)]
pub(super) struct SupportId;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct Support {
    #[get_size(size_fn = support_bit_box_size)]
    bits: BitVec,
}

fn support_bit_box_size(bits: &BitVec) -> usize {
    std::mem::size_of_val(bits.as_raw_slice())
}

impl Support {
    pub(super) fn set(&mut self, typevar: TypeVarId) {
        let index = typevar.index();
        if self.bits.len() < index + 1 {
            self.bits.resize(index + 1, false);
        }
        self.bits.set(index, true);
    }
}

impl BitOrAssign<&Self> for Support {
    fn bitor_assign(&mut self, rhs: &Self) {
        if self.bits.len() < rhs.bits.len() {
            self.bits.resize(rhs.bits.len(), false);
        }
        self.bits |= rhs.bits.as_bitslice();
    }
}
