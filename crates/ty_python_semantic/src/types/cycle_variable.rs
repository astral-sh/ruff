use salsa::plumbing::AsId;

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
use crate::types::infer::constraints::{InferenceOwner, InferenceSlot};
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
        head_id_bits: u64,
        query: CycleQuery<'db>,
        output: CycleOutput<'db>,
    },
    RecursiveType {
        head_id_bits: u64,
    },
    /// An equation reference retained by the inference solver, with no Salsa head.
    Inference {
        program: Program<'db>,
        owner: InferenceOwner<'db>,
        slot: InferenceSlot<'db>,
        specialization: Option<Specialization<'db>>,
    },
}

/// Identity of a query output, recursive approximation, or unresolved inference equation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize, salsa::SalsaValue)]
#[repr(transparent)]
pub struct CycleVariable<'db>(CycleVariableInner<'db>);

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct CycleVariableInner<'db> {
    #[returns(copy)]
    origin: CycleOrigin<'db>,
}

impl get_size2::GetSize for CycleVariableInner<'_> {}

impl AsId for CycleVariable<'_> {
    fn as_id(&self) -> salsa::Id {
        self.0.as_id()
    }
}

impl<'db> CycleVariable<'db> {
    /// Identifies an output supplied by Salsa's cycle recovery.
    pub(crate) fn query(
        db: &'db dyn Db,
        head_id: salsa::Id,
        query: CycleQuery<'db>,
        output: CycleOutput<'db>,
    ) -> Self {
        Self(CycleVariableInner::new(
            db,
            CycleOrigin::Query {
                head_id_bits: head_id.as_bits(),
                query,
                output,
            },
        ))
    }

    /// Marks a recursive type component after its query reference has been cut.
    pub(crate) fn recursive(db: &'db dyn Db, head_id: salsa::Id) -> Self {
        Self(CycleVariableInner::new(
            db,
            CycleOrigin::RecursiveType {
                head_id_bits: head_id.as_bits(),
            },
        ))
    }

    /// Creates a solver reference without constructing a Python type parameter.
    pub(crate) fn inferred(
        db: &'db dyn Db,
        program: Program<'db>,
        owner: InferenceOwner<'db>,
        slot: InferenceSlot<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        Self(CycleVariableInner::new(
            db,
            CycleOrigin::Inference {
                program,
                owner,
                slot,
                specialization,
            },
        ))
    }

    pub(crate) fn origin(self, db: &'db dyn Db) -> CycleOrigin<'db> {
        self.0.origin(db)
    }

    pub(crate) fn head_id(self, db: &'db dyn Db) -> Option<salsa::Id> {
        match self.origin(db) {
            CycleOrigin::Query { head_id_bits, .. }
            | CycleOrigin::RecursiveType { head_id_bits } => {
                Some(salsa::Id::from_bits(head_id_bits))
            }
            CycleOrigin::Inference { .. } => None,
        }
    }
}
