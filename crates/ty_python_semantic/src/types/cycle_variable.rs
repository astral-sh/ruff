use ty_python_core::ExpressionNodeKey;
use ty_python_core::definition::Definition;
use ty_python_core::narrowing_constraints::ScopedNarrowingConstraint;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::predicate::PatternPredicate;
use ty_python_core::scope::ScopeId;
use ty_python_core::statement::StatementInner;
use ty_python_core::unpack::Unpack;

use crate::place::{ConsideredDefinitions, RequiresExplicitReExport};
use crate::types::class::implicit_attributes::ImplicitAttributeName;
use crate::types::generics::Specialization;
use crate::types::infer::{InferExpression, InferScope};
use crate::types::tuple::TupleType;
use crate::types::type_alias::{ManualPEP695TypeAliasType, PEP695TypeAliasType};
use crate::types::{
    BoundTypeVarInstance, FunctionType, MaterializationKind, MemberLookupKey, StaticClassLiteral,
    Type, TypePair, TypeVarInstance,
};
use crate::{Db, Program};

/// The complete inputs of a query that supplies a type during cycle recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum CycleQuery<'db> {
    Definition(Definition<'db>),
    Deferred(Definition<'db>),
    FunctionDefaults(Definition<'db>),
    Scope(InferScope<'db>),
    ExpressionTypes(InferExpression<'db>),
    ExpressionType(InferExpression<'db>),
    Statement(StatementInner<'db>),
    Unpack(Unpack<'db>),
    PublicPlace(
        ScopeId<'db>,
        ScopedPlaceId,
        RequiresExplicitReExport,
        ConsideredDefinitions,
    ),
    ClassMember(MemberLookupKey<'db>),
    Member(MemberLookupKey<'db>),
    MemberWithReceiver(MemberLookupKey<'db>, Type<'db>),
    ImplicitAttribute(ImplicitAttributeName<'db>),
    PreviousPatterns(PatternPredicate<'db>, Type<'db>),
    Pattern(PatternPredicate<'db>, Type<'db>),
    NarrowingCheckpoint(
        ScopeId<'db>,
        ScopedPlaceId,
        ScopedNarrowingConstraint,
        Type<'db>,
    ),
    PatternSuccess(PatternPredicate<'db>),
    Materialization(Type<'db>, Program<'db>, MaterializationKind),
    Specialization(Type<'db>, Specialization<'db>, bool),
    EagerExpansion(Type<'db>, Program<'db>),
    UnionPair(TypePair<'db>),
    IntersectionPair(TypePair<'db>),
    Pep695AliasRaw(PEP695TypeAliasType<'db>),
    ManualAliasRaw(ManualPEP695TypeAliasType<'db>),
    TypeVarDefault(TypeVarInstance<'db>),
    BoundTypeVarDefault(BoundTypeVarInstance<'db>),
    ParameterDefault(Definition<'db>),
    FunctionSignature(FunctionType<'db>),
    TupleClass(TupleType<'db>),
    ExplicitBases(StaticClassLiteral<'db>),
    #[cfg(test)]
    Test,
}

/// A type within a query result, identified without depending on inference order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum CycleOutput<'db> {
    Type,
    Expression(ExpressionNodeKey),
    Binding(Definition<'db>),
    Declaration(Definition<'db>),
    UnpackTarget(ExpressionNodeKey),
    PatternBinding(ScopedPlaceId),
    BindingTypeArgument(Definition<'db>, usize),
    ExplicitBase(usize),
    SignatureReturn,
    TupleElement,
}

/// Distinguishes a query reference from a terminal approximation of a recursive type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum CycleOrigin<'db> {
    Query {
        query: CycleQuery<'db>,
        output: CycleOutput<'db>,
    },
    RecursiveType,
}

/// An unresolved query output or a terminal approximation of a recursive type.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct CycleVariable<'db> {
    /// Salsa's head identity is shared by queries using the same interned input.
    #[returns(copy)]
    head_id_bits: u64,
    #[returns(copy)]
    pub(crate) origin: CycleOrigin<'db>,
}

impl get_size2::GetSize for CycleVariable<'_> {}

impl<'db> CycleVariable<'db> {
    /// Preserve the Salsa head separately from the typed origin of this marker.
    pub(crate) fn new(db: &'db dyn Db, head_id: salsa::Id, origin: CycleOrigin<'db>) -> Self {
        Self::new_internal(db, head_id.as_bits(), origin)
    }

    pub(crate) fn head_id(self, db: &'db dyn Db) -> salsa::Id {
        salsa::Id::from_bits(self.head_id_bits(db))
    }
}
