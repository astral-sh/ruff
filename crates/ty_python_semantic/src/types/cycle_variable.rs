//! Identities of values whose types are still being inferred across a query cycle.
//!
//! A cycle marker (`Divergent`) names the query output it stands for. Later inference steps can
//! then recover that output instead of treating the marker as an opaque dynamic type.

use std::cmp::Ordering;

use salsa::plumbing::AsId;
use ty_python_core::ExpressionNodeKey;
use ty_python_core::unpack::Unpack;

use crate::Db;
use crate::types::class::implicit_attributes::ImplicitAttributeName;
use crate::types::infer::InferenceRegion;
use crate::types::{MemberLookupKey, Type};

/// The query whose result a cycle variable belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, salsa::SalsaValue)]
pub(crate) enum CycleOwner<'db> {
    /// A query identified only by its Salsa key. Its cycle marker is approximated by the
    /// ordinary fixed-point iteration and is never resolved from the query's own result.
    Query(salsa::Id),
    Region(InferenceRegion<'db>),
    Member(MemberLookupKey<'db>, Option<Type<'db>>),
    Attribute(ImplicitAttributeName<'db>),
    Unpack(Unpack<'db>),
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CycleOwner<'_> {}

/// Which output of the owning query a cycle variable denotes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum CycleSlot {
    /// The query's own result.
    Root,
    /// The value of an expression, including an unpacking target.
    Expression(ExpressionNodeKey),
}

/// The Salsa key of the query that seeded a cycle marker.
///
/// Queries keyed by the same value share it, matching how cycle heads are identified during
/// cycle recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, salsa::SalsaValue)]
pub(crate) struct CycleHead(salsa::Id);

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CycleHead {}

impl CycleHead {
    pub(crate) const fn id(self) -> salsa::Id {
        self.0
    }
}

/// A value whose type is still being inferred because its query is part of a cycle.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct CycleVariable<'db> {
    #[returns(copy)]
    pub(crate) head: CycleHead,
    #[returns(copy)]
    pub(crate) owner: CycleOwner<'db>,
    #[returns(copy)]
    pub(crate) slot: CycleSlot,
    /// For a variable standing for the result of an operation, the variable the operation was
    /// applied to.
    #[returns(copy)]
    pub(crate) input: Option<CycleVariable<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for CycleVariable<'_> {}

impl<'db> CycleVariable<'db> {
    pub(crate) fn root(db: &'db dyn Db, head: salsa::Id, owner: CycleOwner<'db>) -> Self {
        Self::new(db, CycleHead(head), owner, CycleSlot::Root, None)
    }

    /// The result of an operation on the value of `input`, recorded by the query `owner` for
    /// the output `slot`.
    pub(crate) fn derived(
        db: &'db dyn Db,
        owner: CycleOwner<'db>,
        slot: CycleSlot,
        input: Self,
    ) -> Self {
        Self::new(db, input.head(db), owner, slot, Some(input))
    }
}

// Deferred operations are stored in ordered maps keyed by their variable. Any total order works
// for a lookup table; ids are stable within a revision.
impl PartialOrd for CycleVariable<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CycleVariable<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_id().cmp(&other.as_id())
    }
}
