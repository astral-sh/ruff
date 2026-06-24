//! Projection artifact data structures.
//!
//! This module only defines the copyable values that represent a cycle root and
//! the operations applied to it. It does not infer or solve projection results.

use ruff_python_ast::name::Name;

use crate::Db;
use crate::types::instance::SliceLiteral;
use crate::types::{DivergentType, KnownClass, MemberLookupPolicy, Type};

/// A projected view of a cycle root produced while recovering recursive inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update)]
pub struct ProjectionType<'db>(ProjectionTypeInterned<'db>);

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionType<'_> {}

impl<'db> ProjectionType<'db> {
    pub(super) fn new(db: &'db dyn Db, root: DivergentType, path: ProjectionPath<'db>) -> Self {
        Self(ProjectionTypeInterned::new(db, root, path))
    }

    pub(crate) fn root(self, db: &'db dyn Db) -> DivergentType {
        self.0.root(db)
    }

    pub(super) fn path(self, db: &'db dyn Db) -> ProjectionPath<'db> {
        self.0.path(db)
    }

    pub(super) fn append(self, db: &'db dyn Db, op: ProjectionOp<'db>) -> Self {
        Self::new(db, self.root(db), self.path(db).append(op))
    }
}

/// Interned storage for [`ProjectionType`].
// Due to salsa restrictions, it is not possible to directly intern a public struct containing a private type.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct ProjectionTypeInterned<'db> {
    root: DivergentType,
    path: ProjectionPath<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionTypeInterned<'_> {}

/// An ordered sequence of projection operations applied to a cycle root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct ProjectionPath<'db> {
    ops: Box<[ProjectionOp<'db>]>,
}

impl<'db> ProjectionPath<'db> {
    pub(super) fn from_op(op: ProjectionOp<'db>) -> Self {
        Self::from_ops([op])
    }

    pub(super) fn from_ops(ops: impl IntoIterator<Item = ProjectionOp<'db>>) -> Self {
        Self {
            ops: ops.into_iter().collect::<Vec<_>>().into_boxed_slice(),
        }
    }

    pub(super) fn ops(&self) -> &[ProjectionOp<'db>] {
        &self.ops
    }

    pub(super) fn append(&self, op: ProjectionOp<'db>) -> Self {
        let mut ops = self.ops.to_vec();
        ops.push(op);
        Self {
            ops: ops.into_boxed_slice(),
        }
    }

    pub(super) fn append_path(&self, path: &Self) -> Self {
        Self::from_ops(self.ops.iter().chain(path.ops.iter()).copied())
    }

    pub(super) fn is_strict_prefix_of(&self, other: &Self) -> bool {
        self.ops.len() < other.ops.len() && other.ops.starts_with(&self.ops)
    }
}

/// An interned member name used by attribute and method-call projections.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub(super) struct ProjectionMemberName<'db> {
    #[returns(ref)]
    name: Name,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionMemberName<'_> {}

impl<'db> ProjectionMemberName<'db> {
    pub(super) fn new(db: &'db dyn Db, name: &Name) -> Self {
        let mut name = name.clone();
        name.shrink_to_fit();
        Self::new_internal(db, name)
    }

    pub(super) fn as_name(self, db: &'db dyn Db) -> &'db Name {
        self.name(db)
    }
}

/// An attribute lookup projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct ProjectionMember<'db> {
    name: ProjectionMemberName<'db>,
    policy: ProjectionMemberLookupPolicy,
}

impl<'db> ProjectionMember<'db> {
    pub(super) fn new(db: &'db dyn Db, name: &Name, policy: MemberLookupPolicy) -> Self {
        Self {
            name: ProjectionMemberName::new(db, name),
            policy: ProjectionMemberLookupPolicy::new(policy),
        }
    }

    pub(super) fn name(self, db: &'db dyn Db) -> &'db Name {
        self.name.as_name(db)
    }

    pub(super) fn policy(self) -> MemberLookupPolicy {
        self.policy.to_policy()
    }
}

/// Compact copyable member lookup policy stored in projection paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
struct ProjectionMemberLookupPolicy(u8);

impl ProjectionMemberLookupPolicy {
    const fn new(policy: MemberLookupPolicy) -> Self {
        Self(policy.bits())
    }

    fn to_policy(self) -> MemberLookupPolicy {
        MemberLookupPolicy::from_bits_retain(self.0)
    }
}

/// An interned non-index subscript key type.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(super) struct ProjectionSubscriptKeyType<'db> {
    ty: Type<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for ProjectionSubscriptKeyType<'_> {}

/// A single operation that can be preserved through cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) enum ProjectionOp<'db> {
    Iter { is_async: bool },
    Unpack(UnpackProjection),
    Subscript(ProjectionSubscript<'db>),
    Member(ProjectionMember<'db>),
    // There is no reason to target only 0-argument methods.
    // It would be nice to be able to scale it without compromising performance.
    CallMethod0(ProjectionMemberName<'db>),
    ContextEnter { is_async: bool },
    AwaitResult,
}

/// The fixed-length or starred-unpack projection of one unpacked position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) enum UnpackProjection {
    Exact {
        len: usize,
        index: usize,
    },
    Star {
        prefix: usize,
        suffix: usize,
        position: StarUnpackPosition,
    },
}

/// A subscript projection represented precisely enough for cycle recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) enum ProjectionSubscript<'db> {
    Unknown,
    Int,
    LiteralInt(i64),
    StaticSlice(StaticSliceProjection),
    KeyType(ProjectionSubscriptKeyType<'db>),
}

impl<'db> ProjectionSubscript<'db> {
    pub(super) fn from_type(db: &'db dyn Db, slice_ty: Type<'db>) -> Option<Self> {
        if let Some(index) = slice_ty.as_int_like_literal() {
            return Some(Self::LiteralInt(index));
        }

        if let Some(slice) = slice_ty
            .as_nominal_instance()
            .and_then(|instance| instance.slice_literal(db))
            && slice.step != Some(0)
        {
            return Some(Self::StaticSlice(StaticSliceProjection::from(slice)));
        }

        if slice_ty.is_instance_of(db, KnownClass::Int)
            || slice_ty.is_instance_of(db, KnownClass::Bool)
        {
            return Some(Self::Int);
        }

        if slice_ty.is_dynamic() {
            return Some(Self::Unknown);
        }

        if slice_ty.is_instance_of(db, KnownClass::Slice) {
            return None;
        }

        Some(Self::KeyType(ProjectionSubscriptKeyType::new(db, slice_ty)))
    }

    pub(super) fn to_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            Self::Unknown => Type::unknown(),
            Self::Int => KnownClass::Int.to_instance(db),
            Self::LiteralInt(index) => Type::int_literal(index),
            Self::StaticSlice(slice) => KnownClass::Slice.to_specialized_instance(
                db,
                &[
                    slice.start.map_or_else(
                        || Type::none(db),
                        |value| Type::int_literal(i64::from(value)),
                    ),
                    slice.stop.map_or_else(
                        || Type::none(db),
                        |value| Type::int_literal(i64::from(value)),
                    ),
                    slice.step.map_or_else(
                        || Type::none(db),
                        |value| Type::int_literal(i64::from(value)),
                    ),
                ],
            ),
            Self::KeyType(key) => key.ty(db),
        }
    }
}

/// The projected position within a starred unpack pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) enum StarUnpackPosition {
    Prefix(usize),
    Rest,
    Suffix(usize),
}

/// A statically known `slice` value used by a subscript projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(super) struct StaticSliceProjection {
    pub(super) start: Option<i32>,
    pub(super) stop: Option<i32>,
    pub(super) step: Option<i32>,
}

impl From<SliceLiteral> for StaticSliceProjection {
    fn from(slice: SliceLiteral) -> Self {
        Self {
            start: slice.start,
            stop: slice.stop,
            step: slice.step,
        }
    }
}
