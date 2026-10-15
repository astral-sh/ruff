//! Cycle detection for recursive types.
//!
//! The visitors here ([`TypeTransformer`] and [`PairVisitor`]) are used in methods that
//! recursively visit types to transform them (e.g. [`Type::apply_type_mapping`]) or to
//! decide a relation between a pair of types (e.g. [`Type::has_relation_to`]).
//!
//! The typical pattern is that the "entry" method (e.g. [`Type::apply_type_mapping`]) will create
//! a visitor and pass it to the recursive method (e.g. [`Type::apply_type_mapping_impl`]).
//! Rust types that form part of a complex type (e.g. tuples, protocols, nominal instances, etc)
//! should usually just implement the recursive method, and all recursive calls should call the
//! recursive method and pass along the visitor.
//!
//! Not all recursive calls need to actually call `.visit` on the visitor; only when visiting types
//! that can create a recursive relationship (this includes, for example, type aliases and
//! protocols).
//!
//! There is a risk of double-visiting, for example if [`Type::apply_type_mapping_impl`] calls
//! `visitor.visit` when visiting a protocol type, and then internal `apply_type_mapping_impl`
//! methods of the Rust types implementing protocols also call `visitor.visit`. The best way to
//! avoid this is to prefer always calling `visitor.visit` only in the main recursive method on
//! `Type`.

use crate::types::TypeOperations;
use crate::types::operations::{RecursiveOperationScope, TypeOperationIdentity};
use crate::types::recursive::{TypeTraversal, TypeWalk, TypeWalkKey, TypeWalkVisit};
use crate::types::visitor::{SmallSet, TypeVisit, TypeVisitorContext, TypeVisitorHooks};
use std::cell::{Cell, OnceCell, RefCell};
use std::cmp::Eq;
use std::collections::hash_map::Entry;
use std::fmt;
use std::hash::Hash;
use std::marker::PhantomData;
use std::mem;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use ty_python_core::definition::Definition;

use crate::types::callable::{
    LambdaSignature, LambdaSource, SignatureSource, infer_lambda_signature,
};
use crate::types::function::{FunctionLiteral, FunctionType};
use crate::types::generics::{GenericContext, Specialization};
use crate::types::list_members::all_end_of_scope_members;
use crate::types::signatures::{CallableSignature, walk_signature};
use crate::types::visitor::{TypeCollector, TypeVisitor, any_over_type_including_alias_arguments};
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, ClassLiteral, KnownClass, MaterializationKind,
    ProtocolInstanceType, RecursiveType, StaticClassLiteral, SubclassOfInner, Type, TypeAliasType,
    TypedDictType,
};
use crate::{Db, ProgramEnvironment};

mod identity;
use identity::StructuralTypeIdentity;

/// A borrow of a recursion guard; only guard implementations can create this capability.
pub(super) struct GuardToken<'guard>(PhantomData<&'guard ()>);

impl<'guard> GuardToken<'guard> {
    fn new<G>(_guard: &'guard G) -> Self {
        Self(PhantomData)
    }
}

/// A shared guard's current state and the observations made by semantic operations.
/// Cache validation reads the state without becoming a dependency itself.
#[derive(Clone, Copy)]
pub(super) struct RecursionGuardState<'a> {
    pub(super) revision: u64,
    pub(super) observations: &'a Cell<u64>,
}

impl RecursionGuardState<'_> {
    pub(super) fn observations(self) -> u64 {
        self.observations.get()
    }

    pub(super) fn observe(self) {
        self.observations
            .set(self.observations.get().wrapping_add(1));
    }
}

/// The type identity used for recursive checks/transformations.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum TypeIdentity<'db> {
    FunctionLiteral(FunctionLiteral<'db>),
    NewTypeInstance(Definition<'db>),
    GrowingNominalInstance(Definition<'db>),
    GrowingClassMembers {
        definition: Definition<'db>,
        subclass: bool,
    },
    GrowingProtocol(Definition<'db>),
    GrowingTypeAlias(Definition<'db>),
    GrowingTypedDict(Definition<'db>),
    GrowingConstructor {
        definition: Definition<'db>,
        runtime_class: Option<KnownClass>,
        /// Descriptors can distinguish an exact class object from a value of `type[C]`.
        subclass: bool,
    },
    GrowingCallableInstance(Definition<'db>, Option<KnownClass>),
    GrowingRecursive {
        definition: Definition<'db>,
        materialization: Option<MaterializationKind>,
        argument_materialization: Option<MaterializationKind>,
    },
    Structural(StructuralTypeIdentity<'db>),
    /// The source and the runtime class required by this callable view.
    GrowingLambda(LambdaSignature<'db>, Option<KnownClass>),
    Other(Type<'db>),
}

impl<'db> Type<'db> {
    pub(crate) fn to_type_identity(self, db: &'db dyn Db) -> TypeIdentity<'db> {
        identity::type_identity(db, self)
    }

    /// Returns `false` if `self` and `other` cannot have the same [`TypeIdentity`].
    ///
    /// A `true` result is only a candidate match and must be confirmed with
    /// [`Type::to_type_identity`].
    pub(crate) fn may_share_type_identity(self, db: &'db dyn Db, other: Self) -> bool {
        if self == other {
            return true;
        }
        match (self, other) {
            (Type::FunctionLiteral(a), Type::FunctionLiteral(b)) => a.literal(db) == b.literal(db),
            (Type::NewTypeInstance(a), Type::NewTypeInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            (Type::NominalInstance(a), Type::NominalInstance(b)) => {
                match (a.stored_class(db), b.stored_class(db)) {
                    (Some(a), Some(b)) => a.class_literal(db) == b.class_literal(db),
                    (None, None) => {
                        a.own_tuple_spec(db).is_some() && b.own_tuple_spec(db).is_some()
                    }
                    _ => false,
                }
            }
            (Type::GenericAlias(a), Type::GenericAlias(b)) => a.origin(db) == b.origin(db),
            (Type::SubclassOf(a), Type::SubclassOf(b)) => {
                Type::from(a).may_share_type_identity(db, Type::from(b))
            }
            (Type::ProtocolInstance(a), Type::ProtocolInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            (Type::TypeAlias(a), Type::TypeAlias(b)) => a.definition(db) == b.definition(db),
            (Type::TypedDict(a), Type::TypedDict(b)) => a.definition(db) == b.definition(db),
            (Type::Recursive(a), Type::Recursive(b)) => {
                a.definition(db) == b.definition(db)
                    && a.materialization_kind(db) == b.materialization_kind(db)
                    && a.arguments(db)
                        .and_then(|args| args.materialization_kind(db))
                        == b.arguments(db)
                            .and_then(|args| args.materialization_kind(db))
            }
            (Type::Callable(a), Type::Callable(b)) => {
                if a.runtime_class(db) != b.runtime_class(db) {
                    return false;
                }
                matches!(
                    (a.signature_source(db), b.signature_source(db)),
                    (SignatureSource::Explicit(_), SignatureSource::Explicit(_))
                ) || matches!(
                    (a.signature_source(db), b.signature_source(db)),
                    (SignatureSource::Lambda(a), SignatureSource::Lambda(b))
                        if a.source(db) == b.source(db)
                ) || matches!((a.signature_source(db), b.signature_source(db)),
                        (SignatureSource::Projected(a), SignatureSource::Projected(b))
                        if RecursiveDefinition::from_type(db, a.source()).zip(RecursiveDefinition::from_type(db, b.source()))
                            .is_some_and(|(a,b)| a.target(db).origin(db) == b.target(db).origin(db)))
            }
            _ => mem::discriminant(&self) == mem::discriminant(&other),
        }
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn recursive_identity(self, db: &'db dyn Db) -> Option<TypeIdentity<'db>> {
        match self {
            // We can create a self-referential function type: e.g. `def f(x: "TypeOf[f]"): reveal_type(x)`
            // To avoid the difficulty of equality checking for function types containing this, we simply use `literal` for equality checking.
            Type::FunctionLiteral(function) => {
                Some(TypeIdentity::FunctionLiteral(function.literal(db)))
            }
            // Similarly, we can create a self-referential NewType: e.g. `T = NewType("T", list["T"])`
            Type::NewTypeInstance(newtype) => {
                Some(TypeIdentity::NewTypeInstance(newtype.definition(db)))
            }
            // Recursive types whose arguments can grow without bound
            // (e.g. `type Growing[T] = T | Growing[list[T]]`) use their source as an identity so
            // that visits stop even though no exact type repeats. Recursion that revisits one
            // exact specialization (e.g. `type RecursiveT = int | tuple[RecursiveT, ...]`) needs
            // no coarser identity: the detectors stop on the repeated type itself.
            Type::NominalInstance(_)
            | Type::GenericAlias(_)
            | Type::SubclassOf(_)
            | Type::TypeAlias(_)
            | Type::ProtocolInstance(_)
            | Type::TypedDict(_)
            | Type::Recursive(_)
            | Type::Callable(_) => {
                let reference = RecursiveDefinition::from_type(db, self)?;
                let target = if matches!(self, Type::GenericAlias(_) | Type::SubclassOf(_)) {
                    reference.for_member_access().target(db)
                } else {
                    reference.target(db)
                };
                if !target.may_have_unbounded_specialization(db) {
                    return None;
                }
                Some(match target {
                    RecursiveDefinition::Nominal(class) => {
                        TypeIdentity::GrowingNominalInstance(class.definition(db))
                    }
                    RecursiveDefinition::ClassMembers(class) => TypeIdentity::GrowingClassMembers {
                        definition: class.definition(db),
                        subclass: matches!(self, Type::SubclassOf(_)),
                    },
                    RecursiveDefinition::TypeAlias(alias) => {
                        TypeIdentity::GrowingTypeAlias(alias.definition(db))
                    }
                    RecursiveDefinition::Protocol(class) => {
                        TypeIdentity::GrowingProtocol(class.definition(db))
                    }
                    RecursiveDefinition::TypedDict(class) => {
                        TypeIdentity::GrowingTypedDict(class.definition(db))
                    }
                    RecursiveDefinition::Constructor(class) => {
                        let callable = self.as_callable()?;
                        let SignatureSource::Projected(projection) = callable.signature_source(db)
                        else {
                            return None;
                        };
                        TypeIdentity::GrowingConstructor {
                            definition: class.definition(db),
                            runtime_class: callable.runtime_class(db),
                            subclass: matches!(projection.source(), Type::SubclassOf(_)),
                        }
                    }
                    // Descriptor reads are graph edges, not standalone type identities.
                    RecursiveDefinition::DescriptorGet(_) => return None,
                    RecursiveDefinition::CallableInstance(class) => {
                        TypeIdentity::GrowingCallableInstance(
                            class.definition(db),
                            self.as_callable()?.runtime_class(db),
                        )
                    }
                    RecursiveDefinition::Structural(_) => {
                        let Type::Recursive(recursive) = self else {
                            return None;
                        };
                        TypeIdentity::GrowingRecursive {
                            definition: recursive.definition(db),
                            materialization: recursive.materialization_kind(db),
                            argument_materialization: recursive
                                .arguments(db)
                                .and_then(|args| args.materialization_kind(db)),
                        }
                    }
                    RecursiveDefinition::Lambda(lambda) => {
                        TypeIdentity::GrowingLambda(lambda, self.as_callable()?.runtime_class(db))
                    }
                })
            }
            _ => None,
        }
    }
}

impl<'db> RecursiveType<'db> {
    /// Whether revisiting this constructor can keep changing its type arguments.
    pub(super) fn may_have_unbounded_specialization(self, db: &'db dyn Db) -> bool {
        matches!(
            Type::Recursive(self).recursive_identity(db),
            Some(TypeIdentity::GrowingRecursive { .. })
        )
    }
}

/// A source whose type parameters can flow through recursive references.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum RecursiveDefinition<'db> {
    Nominal(StaticClassLiteral<'db>),
    ClassMembers(StaticClassLiteral<'db>),
    TypeAlias(TypeAliasType<'db>),
    Protocol(StaticClassLiteral<'db>),
    TypedDict(StaticClassLiteral<'db>),
    Constructor(StaticClassLiteral<'db>),
    CallableInstance(StaticClassLiteral<'db>),
    /// A descriptor read, used only as an intermediate node in the specialization graph.
    DescriptorGet(StaticClassLiteral<'db>),
    Structural(RecursiveType<'db>),
    Lambda(LambdaSignature<'db>),
}

/// The source before specialization. Lambdas retain their original inference inputs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RecursiveOrigin<'db> {
    Definition(Definition<'db>),
    ClassMembers(Definition<'db>),
    Constructor(Definition<'db>),
    CallableInstance(Definition<'db>),
    DescriptorGet(Definition<'db>),
    Lambda(LambdaSignature<'db>),
}

/// A type parameter in one recursive source.
///
/// Two lambdas can capture the same outer variable. Keeping their parameters separate prevents
/// a single reference between them from appearing to be a cycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FlowParameter<'db> {
    origin: RecursiveOrigin<'db>,
    identity: BoundTypeVarIdentity<'db>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum FlowKind {
    /// The source parameter is passed through operations that cannot accumulate structure.
    Direct,
    /// The source parameter occurs inside a type structure that can accumulate.
    Nested,
}

/// Flow from a source parameter to the argument of a referenced type. For example:
/// ```py
/// type A[A1, A2] = B[A2, A1]
/// type B[B1, B2] = None
/// ```
/// produces the flow graph:
/// ```ignore
/// FlowEdge {
///     from: A::A2,
///     to: B::B1,
///     kind: FlowKind::Direct,
/// }
/// FlowEdge {
///     from: A::A1,
///     to: B::B2,
///     kind: FlowKind::Direct,
/// }
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FlowEdge<'db> {
    from: FlowParameter<'db>,
    to: FlowParameter<'db>,
    kind: FlowKind,
}

#[derive(Clone, Copy)]
enum DefinitionUse<'db> {
    Specialized(RecursiveDefinition<'db>, Option<Specialization<'db>>),
    Lambda(LambdaSignature<'db>),
}

/// Parameter flow between all recursive definitions reachable from one root.
///
/// Whether a recursive definition can keep producing new specializations is modeled as a graph
/// problem. The formal parameters of every reachable definition are the nodes, and each argument
/// of a recursive reference adds an edge to the parameter it specializes from every source
/// parameter occurring in it: [`FlowKind::Direct`] for direct references, normalized set operations,
/// and metaclass projections, and
/// [`FlowKind::Nested`] if it occurs inside a type structure that can accumulate. Arguments
/// without source parameters add no edges and act as resets.
///
/// Each expansion step moves the argument types along these edges, so the root's specialization
/// can grow without bound only if a directed cycle through one of the root's parameters contains
/// a nested edge:
///
/// ```py
/// type Growing[X, Y] = Growing[list[Y], X]        # Y -> X nested, X -> Y direct: growing cycle
/// type Shifting[A, B, C] = Shifting[B, C, None]   # direct edges only: repeats after 3 steps
/// type Resetting[X, Y] = Resetting[list[Y], None] # Y -> X nested, but X flows nowhere: no cycle
/// type GrowingOuter[T] = GrowingHelper[list[T]]   # T -> U nested
/// type GrowingHelper[U] = GrowingOuter[U]         # U -> T direct: the growing cycle spans both
/// type ResetOuter[T] = ResetHelper[list[T]]       # T -> U nested
/// type ResetHelper[U] = ResetOuter[int]           # parameter-free argument: no edge, no cycle
/// ```
///
/// Without such a cycle, every parameter's value stays within the finite set of types built from
/// the initial arguments and reset types by normalized set operations, so the expansions reach an
/// exact repetition and the cycle detectors can rely on exact type identities.
#[derive(Default)]
pub(crate) struct SpecializationFlowGraph<'db> {
    edges: FxHashSet<FlowEdge<'db>>,
    /// Definition references used to decide whether an unresolved flow can return to the root.
    definition_edges: Vec<(RecursiveOrigin<'db>, RecursiveOrigin<'db>)>,
    /// Definitions whose parameter bindings cannot be represented in the graph.
    inconclusive_definitions: FxHashSet<RecursiveOrigin<'db>>,
    /// Whether a definition body or its formal parameters could not be inspected.
    inconclusive: bool,
}

/// Walks one identity-specialized definition body and records references as graph edges.
///
/// Referenced definitions are queued for a separate walk instead of being expanded here.
struct SpecializationFlowVisitor<'db> {
    source_origin: RecursiveOrigin<'db>,
    source_parameters: FxHashSet<BoundTypeVarIdentity<'db>>,
    env: TypeOperations<'db>,
    visited_types: TypeCollector<'db>,
    edges: RefCell<Vec<FlowEdge<'db>>>,
    referenced_definitions: RefCell<Vec<RecursiveDefinition<'db>>>,
    inconclusive: Cell<bool>,
    has_unresolved_type: Cell<bool>,
}

/// Finds which parameters of the current source definition occur in one actual argument.
struct SourceParameterCollector<'a, 'db> {
    source_parameters: &'a FxHashSet<BoundTypeVarIdentity<'db>>,
    env: &'a TypeOperations<'db>,
    found: RefCell<FxHashMap<BoundTypeVarIdentity<'db>, bool>>,
    visited_types: TypeCollector<'db>,
    in_nested_type: Cell<bool>,
}

impl<'db> RecursiveDefinition<'db> {
    fn from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<DefinitionUse<'db>> {
        let (target, specialization) = match ty {
            Type::ClassLiteral(ClassLiteral::Static(class)) => {
                class.generic_context(db)?;
                (Self::Constructor(class), None)
            }
            Type::GenericAlias(alias) => (
                Self::Constructor(alias.origin(db)),
                Some(alias.specialization(db)),
            ),
            Type::SubclassOf(subclass) => {
                let SubclassOfInner::Class(class) = subclass.subclass_of() else {
                    return None;
                };
                let (class, arguments) = class.static_class_literal(db)?;
                class.generic_context(db)?;
                (Self::Constructor(class), arguments)
            }
            Type::NominalInstance(instance) => {
                let (class, arguments) = instance.stored_class(db)?.static_class_literal(db)?;
                class.generic_context(db)?;
                (Self::Nominal(class), arguments)
            }
            Type::Recursive(recursive) => {
                let specialization = recursive.arguments(db)?;
                (
                    Self::Structural(recursive.constructor(db)),
                    Some(specialization),
                )
            }
            Type::TypeAlias(alias) => (
                Self::TypeAlias(alias.unspecialized(db)),
                alias.specialization(db),
            ),
            Type::ProtocolInstance(protocol) => {
                let (origin, specialization) =
                    protocol.class_origin(db)?.static_class_literal(db)?;
                (Self::Protocol(origin), specialization)
            }
            Type::TypedDict(typed_dict) => {
                let (origin, specialization) =
                    typed_dict.defining_class()?.static_class_literal(db)?;
                (Self::TypedDict(origin), specialization)
            }
            Type::Callable(callable) => {
                return match callable.signature_source(db) {
                    SignatureSource::Lambda(lambda) => Some(DefinitionUse::Lambda(*lambda)),
                    SignatureSource::Projected(projection) => {
                        Self::from_type(db, projection.source()).map(DefinitionUse::for_call)
                    }
                    SignatureSource::Explicit(_) => None,
                };
            }
            _ => return None,
        };

        Some(DefinitionUse::Specialized(target, specialization))
    }

    fn origin(self, db: &'db dyn Db) -> RecursiveOrigin<'db> {
        match self {
            Self::TypeAlias(alias) => RecursiveOrigin::Definition(alias.definition(db)),
            Self::Structural(recursive) => RecursiveOrigin::Definition(recursive.definition(db)),
            Self::Nominal(origin) | Self::Protocol(origin) | Self::TypedDict(origin) => {
                RecursiveOrigin::Definition(origin.definition(db))
            }
            Self::ClassMembers(class) => RecursiveOrigin::ClassMembers(class.definition(db)),
            Self::Lambda(lambda) => RecursiveOrigin::Lambda(lambda),
            Self::Constructor(class) => RecursiveOrigin::Constructor(class.definition(db)),
            Self::CallableInstance(class) => {
                RecursiveOrigin::CallableInstance(class.definition(db))
            }
            Self::DescriptorGet(class) => RecursiveOrigin::DescriptorGet(class.definition(db)),
        }
    }

    fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        match self.origin(db) {
            RecursiveOrigin::Definition(definition)
            | RecursiveOrigin::ClassMembers(definition)
            | RecursiveOrigin::Constructor(definition)
            | RecursiveOrigin::CallableInstance(definition)
            | RecursiveOrigin::DescriptorGet(definition) => {
                ProgramEnvironment::from_definition(definition)
            }
            RecursiveOrigin::Lambda(lambda) => ProgramEnvironment::from_scope(lambda.scope(db)),
        }
    }

    fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        match self {
            Self::TypeAlias(alias) => alias.generic_context(db),
            Self::Structural(recursive) => recursive.parameters(db),
            Self::Nominal(origin)
            | Self::ClassMembers(origin)
            | Self::Protocol(origin)
            | Self::TypedDict(origin)
            | Self::Constructor(origin)
            | Self::CallableInstance(origin)
            | Self::DescriptorGet(origin) => origin.generic_context(db),
            Self::Lambda(lambda) => Some(LambdaSource::of(db, lambda).captured_typevars(db)),
        }
    }

    fn default_specialization(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        generic_context: GenericContext<'db>,
    ) -> Specialization<'db> {
        let known_class = match self {
            Self::TypeAlias(_) | Self::Structural(_) | Self::Lambda(_) => None,
            Self::Nominal(origin)
            | Self::ClassMembers(origin)
            | Self::Protocol(origin)
            | Self::TypedDict(origin)
            | Self::Constructor(origin)
            | Self::CallableInstance(origin)
            | Self::DescriptorGet(origin) => origin.known(db),
        };
        generic_context.default_specialization(db, env, known_class)
    }

    fn parameter_identity(
        db: &'db dyn Db,
        parameter: BoundTypeVarInstance<'db>,
    ) -> BoundTypeVarIdentity<'db> {
        let identity = parameter.identity(db);
        if identity.is_paramspec(db) {
            identity.without_paramspec_attr(db)
        } else {
            identity
        }
    }

    /// The identities of this definition's formal parameters, in declaration order.
    fn parameters(self, db: &'db dyn Db) -> impl Iterator<Item = FlowParameter<'db>> {
        self.generic_context(db)
            .into_iter()
            .flat_map(|context| context.variables(db))
            .map(move |parameter| FlowParameter {
                origin: self.origin(db),
                identity: Self::parameter_identity(db, parameter),
            })
    }

    /// Returns `None` if two formal parameters share an identity, since their flows could not be
    /// distinguished.
    fn source_parameters(self, db: &'db dyn Db) -> Option<FxHashSet<BoundTypeVarIdentity<'db>>> {
        let mut parameters = FxHashSet::default();
        for parameter in self.parameters(db) {
            if !parameters.insert(parameter.identity) {
                return None;
            }
        }
        Some(parameters)
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "The tracked query uses None as its initial cycle value"
    )]
    fn may_have_unbounded_specialization(self, db: &'db dyn Db) -> bool {
        // An unfinished definition cannot prove that specialization is finite. Once an
        // iteration finds growth, retain that evidence even if another body approximation
        // temporarily hides the recursive edge.
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, _, _, ()| None,
            cycle_fn=|_, _, previous: &Option<bool>, current: Option<bool>, _, ()| {
                Some(previous.unwrap_or(false) || current.unwrap_or(false))
            },
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn may_have_unbounded_specialization_inner<'db>(
            db: &'db dyn Db,
            root: RecursiveDefinition<'db>,
            _: (),
        ) -> Option<bool> {
            let graph = SpecializationFlowGraph::build(db, root);
            Some(graph.root_may_have_unbounded_specialization(db, root))
        }

        may_have_unbounded_specialization_inner(db, self, ()).unwrap_or(true)
    }
}

impl<'db> DefinitionUse<'db> {
    /// Class-object lookup includes metaclass members as well as the class's declarations.
    fn for_member_access(self) -> Self {
        match self {
            Self::Specialized(RecursiveDefinition::Constructor(class), arguments) => {
                Self::Specialized(RecursiveDefinition::ClassMembers(class), arguments)
            }
            Self::Specialized(..) | Self::Lambda(_) => self,
        }
    }

    /// Calling an instance inspects its `__call__` member rather than its ordinary attributes.
    fn for_call(self) -> Self {
        match self {
            Self::Specialized(RecursiveDefinition::Nominal(class), arguments) => {
                Self::Specialized(RecursiveDefinition::CallableInstance(class), arguments)
            }
            Self::Specialized(..) | Self::Lambda(_) => self,
        }
    }

    fn target(self, db: &'db dyn Db) -> RecursiveDefinition<'db> {
        match self {
            Self::Specialized(target, _) => target,
            Self::Lambda(lambda) => RecursiveDefinition::Lambda(lambda.source(db)),
        }
    }

    /// Read actual arguments without expanding the referenced definition's body.
    fn arguments(
        self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
    ) -> Option<Vec<(BoundTypeVarIdentity<'db>, Type<'db>)>> {
        let (target, specialization) = match self {
            Self::Specialized(target, specialization) => (target, specialization),
            Self::Lambda(lambda) => {
                return Some(
                    lambda
                        .captured_arguments(db, env)
                        .flat_map(|(parameter, arguments)| {
                            let identity = RecursiveDefinition::parameter_identity(db, parameter);
                            arguments
                                .into_iter()
                                .map(move |argument| (identity, argument))
                        })
                        .collect(),
                );
            }
        };
        let Some(context) = target.generic_context(db) else {
            return specialization.is_none().then(Vec::new);
        };
        let specialization =
            specialization.unwrap_or_else(|| target.default_specialization(db, env, context));
        if specialization.generic_context(db) != context {
            return None;
        }
        let parameters = context
            .variables(db)
            .map(|parameter| RecursiveDefinition::parameter_identity(db, parameter));
        let arguments = specialization.types(db);
        if parameters.len() != arguments.len() {
            return None;
        }
        Some(parameters.zip(arguments.iter().copied()).collect())
    }

    fn walk_arguments(self, db: &'db dyn Db, visitor: &impl TypeVisitor<'db>) {
        match self {
            Self::Specialized(target, specialization) => {
                let specialization = specialization.or_else(|| {
                    target.generic_context(db).map(|context| {
                        target.default_specialization(db, visitor.program_environment(), context)
                    })
                });
                if let Some(specialization) = specialization {
                    for argument in specialization.types(db) {
                        visitor.visit_type(db, *argument);
                    }
                }
            }
            Self::Lambda(lambda) => {
                for (_, arguments) in lambda.captured_arguments(db, visitor.program_environment()) {
                    for argument in arguments {
                        visitor.visit_type(db, argument);
                    }
                }
            }
        }
    }
}

impl<'db> SpecializationFlowGraph<'db> {
    fn build(db: &'db dyn Db, root: RecursiveDefinition<'db>) -> Self {
        let env = TypeOperations::new_root(db, root.environment(db));
        let mut graph = SpecializationFlowGraph::default();
        let mut pending = vec![root];
        let mut visited = FxHashSet::default();

        while let Some(source) = pending.pop() {
            let source_definition = source.origin(db);
            if !visited.insert(source_definition) {
                continue;
            }
            let Some(visitor) = SpecializationFlowVisitor::new(db, &env, source) else {
                graph.inconclusive = true;
                continue;
            };
            if !visitor.visit_definition_body(db, source) {
                graph.inconclusive = true;
            }
            let (edges, referenced_definitions, inconclusive) = visitor.finish();
            graph.edges.extend(edges);
            if inconclusive {
                graph.inconclusive_definitions.insert(source_definition);
            }
            graph.definition_edges.extend(
                referenced_definitions
                    .iter()
                    .map(|target| (source_definition, target.origin(db))),
            );
            pending.extend(referenced_definitions);
        }
        graph
    }

    fn root_may_have_unbounded_specialization(
        &self,
        db: &'db dyn Db,
        root: RecursiveDefinition<'db>,
    ) -> bool {
        if self.inconclusive {
            return true;
        }

        let root_definition = root.origin(db);
        if self.inconclusive_definition_reaches(root_definition) {
            return true;
        }

        if !self.edges.iter().any(|edge| edge.kind == FlowKind::Nested) {
            return false;
        }

        let root_parameters = root.parameters(db).collect::<Vec<_>>();
        let components =
            self.strongly_connected_parameter_components(root_parameters.iter().copied());
        let root_components = root_parameters
            .iter()
            .filter_map(|parameter| components.get(parameter).copied())
            .collect::<FxHashSet<_>>();

        self.edges.iter().any(|edge| {
            if edge.kind != FlowKind::Nested {
                return false;
            }
            components
                .get(&edge.from)
                .zip(components.get(&edge.to))
                .is_some_and(|(from_component, to_component)| {
                    // True if this edge is inside the SCC (a nested cycle is formed).
                    from_component == to_component
                    // Only a nested cycle containing a root parameter can grow the root's
                    // specialization. Helper-only cycles are handled when visiting the helper.
                    && root_components.contains(from_component)
                })
        })
    }

    /// Assigns each parameter to its strongly connected component.
    /// This function returns a map from typevar to the index of the SCC to which it belongs.
    /// This means that typevars with the same index belong to the same SCC.
    fn strongly_connected_parameter_components(
        &self,
        additional_parameters: impl IntoIterator<Item = FlowParameter<'db>>,
    ) -> FxHashMap<FlowParameter<'db>, usize> {
        let mut parameters = additional_parameters.into_iter().collect::<FxHashSet<_>>();
        let mut outgoing = FxHashMap::<_, SmallVec<[_; 2]>>::default();
        let mut incoming = FxHashMap::<_, SmallVec<[_; 2]>>::default();
        #[expect(
            clippy::iter_over_hash_type,
            reason = "component membership is independent of traversal order"
        )]
        for edge in &self.edges {
            parameters.insert(edge.from);
            parameters.insert(edge.to);
            outgoing.entry(edge.from).or_default().push(edge.to);
            incoming.entry(edge.to).or_default().push(edge.from);
        }

        let mut visited = FxHashSet::default();
        let mut finishing_order = Vec::with_capacity(parameters.len());
        #[expect(
            clippy::iter_over_hash_type,
            reason = "component membership is independent of traversal order"
        )]
        for start in parameters {
            if !visited.insert(start) {
                continue;
            }

            let mut pending = vec![(start, 0)];
            while let Some((current, next_index)) = pending.pop() {
                let next = outgoing
                    .get(&current)
                    .and_then(|parameters| parameters.get(next_index))
                    .copied();
                if let Some(next) = next {
                    pending.push((current, next_index + 1));
                    if visited.insert(next) {
                        pending.push((next, 0));
                    }
                } else {
                    finishing_order.push(current);
                }
            }
        }

        let mut components = FxHashMap::default();
        for start in finishing_order.into_iter().rev() {
            if components.contains_key(&start) {
                continue;
            }

            let component = components.len();
            components.insert(start, component);
            let mut pending = vec![start];
            while let Some(current) = pending.pop() {
                if let Some(previous_parameters) = incoming.get(&current) {
                    for &previous in previous_parameters {
                        if let Entry::Vacant(entry) = components.entry(previous) {
                            entry.insert(component);
                            pending.push(previous);
                        }
                    }
                }
            }
        }
        components
    }

    fn inconclusive_definition_reaches(&self, target: RecursiveOrigin<'db>) -> bool {
        if self.inconclusive_definitions.is_empty() {
            return false;
        }

        let definitions_reaching_target = self.definitions_reaching(target);
        !self
            .inconclusive_definitions
            .is_disjoint(&definitions_reaching_target)
    }

    fn definition_reaches(&self, from: RecursiveOrigin<'db>, to: RecursiveOrigin<'db>) -> bool {
        self.definitions_reaching(to).contains(&from)
    }

    fn definitions_reaching(
        &self,
        target: RecursiveOrigin<'db>,
    ) -> FxHashSet<RecursiveOrigin<'db>> {
        let mut incoming =
            FxHashMap::<RecursiveOrigin<'db>, SmallVec<[RecursiveOrigin<'db>; 2]>>::default();
        for &(source, target) in &self.definition_edges {
            incoming.entry(target).or_default().push(source);
        }

        // Start from predecessors rather than the target itself so the result contains only
        // definitions with a non-empty path to the target. The target itself is included only if
        // it belongs to a cycle.
        let mut pending = Vec::new();
        if let Some(sources) = incoming.get(&target) {
            pending.extend(sources.iter().copied());
        }
        let mut visited = FxHashSet::default();
        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }
            if let Some(sources) = incoming.get(&current) {
                pending.extend(sources.iter().copied());
            }
        }
        visited
    }
}

impl<'db> SpecializationFlowVisitor<'db> {
    fn new(
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        source: RecursiveDefinition<'db>,
    ) -> Option<Self> {
        Some(Self {
            source_origin: source.origin(db),
            source_parameters: source.source_parameters(db)?,
            env: env.in_environment(source.environment(db)),
            visited_types: TypeCollector::default(),
            edges: RefCell::default(),
            referenced_definitions: RefCell::default(),
            inconclusive: Cell::default(),
            has_unresolved_type: Cell::default(),
        })
    }

    fn finish(self) -> (Vec<FlowEdge<'db>>, Vec<RecursiveDefinition<'db>>, bool) {
        (
            self.edges.into_inner(),
            self.referenced_definitions.into_inner(),
            self.inconclusive.get(),
        )
    }

    /// Visits the definition with each formal parameter mapped to itself.
    /// Returns `false` if the body is incomplete and could still introduce recursive references.
    fn visit_definition_body(&self, db: &'db dyn Db, source: RecursiveDefinition<'db>) -> bool {
        let env = &self.env;
        let traversal = TypeVisitorContext::new(self);
        match source {
            RecursiveDefinition::Nominal(origin) => self.visit_nominal_members(db, origin),
            RecursiveDefinition::ClassMembers(origin) => {
                self.visit_nominal_members(db, origin);
                let metaclass = origin.identity_specialization(db).metaclass(db, env);
                if let Some((metaclass, arguments)) = metaclass
                    .to_class_type(db, env)
                    .and_then(|class| class.static_class_literal(db))
                {
                    // Metaclasses need not be generic to expose generic properties or descriptors.
                    // Keep this edge separate from instance lookup: Node's instances do not read
                    // attributes from Node's metaclass.
                    let reference = DefinitionUse::Specialized(
                        RecursiveDefinition::Nominal(metaclass),
                        arguments,
                    );
                    self.record_reference(db, reference);
                    reference.walk_arguments(db, &traversal);
                } else {
                    self.has_unresolved_type.set(true);
                }
            }
            RecursiveDefinition::Structural(recursive) => {
                let visitor: super::recursive::TypeVisitorState<'_, 'db, Type<'db>, bool> =
                    super::recursive::TypeVisitorState::new(
                        &self.env,
                        super::recursive::TypeVisitorKey::SpecializationFlow,
                        false,
                    );
                return visitor.visit_structure(db, Type::Recursive(recursive), |expansion| {
                    let Some(body) = expansion.recursive_body(db, recursive).into_unfolded() else {
                        return false;
                    };
                    body.visit(db, &traversal);
                    !self.has_unresolved_type.get()
                });
            }
            RecursiveDefinition::TypeAlias(alias) => {
                if alias
                    .inspect_definition(db, env, |body| body.visit(db, &traversal))
                    .is_none()
                {
                    return false;
                }
            }
            RecursiveDefinition::Protocol(origin) => {
                let Some(protocol) = origin.identity_specialization(db).into_protocol_class(db)
                else {
                    return false;
                };
                protocol.walk_recursive_member_types(db, &traversal);
            }
            RecursiveDefinition::TypedDict(origin) => {
                let typed_dict = TypedDictType::new(origin.identity_specialization(db));
                for field in typed_dict.items(db, env).values() {
                    traversal.visit_type(db, field.declared_ty);
                }
                if let Some(extra_items) = typed_dict.explicit_extra_items(db, env) {
                    traversal.visit_type(db, extra_items.declared_ty);
                }
            }
            RecursiveDefinition::Lambda(lambda) => {
                for signature in infer_lambda_signature(db, LambdaSource::of(db, lambda)) {
                    walk_signature(db, signature, &traversal);
                }
            }
            RecursiveDefinition::Constructor(origin) => {
                let class = origin.identity_specialization(db);
                // Construction can return an instance of this class, whose __call__ may lead
                // back to a constructor. Include that result alongside explicit return types.
                self.record_reference(
                    db,
                    DefinitionUse::Specialized(
                        RecursiveDefinition::CallableInstance(origin),
                        class.class_literal_and_specialization(db).1,
                    ),
                );
                // Read declarations without calling constructors or binding descriptors. Those
                // operations can recurse before this graph has classified their type arguments.
                for name in ["__new__", "__init__"] {
                    if let Some(member) = class
                        .class_member(
                            db,
                            env,
                            name,
                            super::MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                        )
                        .place
                        .ignore_possibly_undefined()
                    {
                        self.visit_callable_member(db, member);
                    }
                }
                if let Some(member) = Type::from(class)
                    .class_member_with_policy(
                        db,
                        env,
                        "__call__",
                        super::MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
                    )
                    .place
                    .ignore_possibly_undefined()
                {
                    self.visit_callable_member(db, member);
                }
            }
            RecursiveDefinition::CallableInstance(class) => {
                if let Some(member) = class
                    .identity_specialization(db)
                    .class_member(
                        db,
                        env,
                        "__call__",
                        super::MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place
                    .ignore_possibly_undefined()
                {
                    self.visit_callable_member(db, member);
                }
            }
            RecursiveDefinition::DescriptorGet(class) => {
                // Python looks up __get__ without applying the descriptor protocol again. A
                // callable-valued __get__ is another graph edge; a function exposes its result.
                if let Some(member) = class
                    .identity_specialization(db)
                    .class_member(
                        db,
                        env,
                        "__get__",
                        super::MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .place
                    .ignore_possibly_undefined()
                {
                    self.visit_accessor_annotation(db, member, None);
                }
            }
        }
        !self.has_unresolved_type.get()
    }

    fn visit_callable_member(&self, db: &'db dyn Db, member: Type<'db>) {
        TypeTraversal::with_root(&self.env, CallableMemberFlow(self), |walk| {
            walk.visit(db, member);
        });
    }

    /// Follow declared instance members without reading descriptors or calling accessors.
    /// Bases contribute their own members through their specialized definition references.
    fn visit_nominal_members(&self, db: &'db dyn Db, origin: StaticClassLiteral<'db>) {
        let traversal = TypeVisitorContext::new(self);
        for base in origin.explicit_bases(db) {
            if let Some(instance) = base.to_instance(db, &self.env) {
                let instance = instance.into_inner();
                if let Type::NominalInstance(instance) = instance
                    && let Some((base, arguments)) = instance
                        .stored_class(db)
                        .and_then(|class| class.static_class_literal(db))
                {
                    // A nongeneric base can still contribute an accessor with method parameters.
                    let reference =
                        DefinitionUse::Specialized(RecursiveDefinition::Nominal(base), arguments);
                    self.record_reference(db, reference);
                    reference.walk_arguments(db, &traversal);
                } else {
                    traversal.visit_type(db, instance);
                }
            }
        }

        let mut seen = FxHashSet::default();
        for member in all_end_of_scope_members(db, &self.env, origin.body_scope(db)) {
            if seen.insert(member.member.name.clone()) {
                self.visit_nominal_member(db, member.member.name.as_str(), member.member.ty);
            }
        }

        let class = origin.identity_specialization(db);
        for name in class.own_instance_attribute_names(db) {
            if seen.insert(name.clone())
                && let Some(ty) = class
                    .own_instance_member(db, &self.env, name)
                    .ignore_possibly_undefined()
            {
                self.visit_nominal_member(db, name, ty);
            }
        }
    }

    fn visit_nominal_member(&self, db: &'db dyn Db, name: &str, ty: Type<'db>) {
        if matches!(name, "__getattr__" | "__getattribute__") {
            self.visit_accessor_annotation(db, ty, None);
            return;
        }
        match ty {
            // Method comparisons already guard their source declarations. Including receivers
            // here would make an otherwise non-recursive generic class appear recursive.
            Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::KnownBoundMethod(_) => {}
            Type::PropertyInstance(property) => {
                if let Some(getter) = property.getter(db) {
                    self.visit_accessor_annotation(db, getter, None);
                }
                if let Some(setter) = property.setter(db) {
                    self.visit_accessor_annotation(db, setter, Some(1));
                }
            }
            _ => self.visit_callable_member(db, ty),
        }
    }

    /// Read signatures without expanding callable projections or applying argument bindings.
    fn source_signatures(db: &'db dyn Db, ty: Type<'db>) -> Option<&'db CallableSignature<'db>> {
        match ty.underlying_function(db) {
            Type::FunctionLiteral(function) => Some(function.signature(db)),
            Type::Callable(callable)
                if let SignatureSource::Explicit(signatures) = callable.signature_source(db) =>
            {
                Some(signatures)
            }
            _ => None,
        }
    }

    /// Inspect source signatures, excluding receivers and implementation-only overload types.
    fn visit_accessor_annotation(
        &self,
        db: &'db dyn Db,
        accessor: Type<'db>,
        parameter: Option<usize>,
    ) {
        let traversal = TypeVisitorContext::new(self);
        let Some(signatures) = Self::source_signatures(db, accessor) else {
            self.has_unresolved_type.set(true);
            traversal.visit_type(db, accessor);
            return;
        };
        for signature in &signatures.overloads {
            let ty = match parameter {
                None => signature.return_ty,
                Some(index) => {
                    let Some(parameter) = signature.parameters().get_positional(index) else {
                        continue;
                    };
                    parameter.annotated_type()
                }
            };
            // Binding an accessor can infer a method parameter from its receiver. The result
            // can then refer back to any class, even when its annotation is only a type variable.
            // Inspect the exposed annotation, not receiver parameters: a receiver such as
            // Wrapper[T] does not itself make the finite projection Wrapper[T] -> T recursive.
            if any_over_type_including_alias_arguments(db, &self.env, ty, |ty| {
                matches!(ty, Type::TypeVar(typevar)
                    if !self.source_parameters.contains(&RecursiveDefinition::parameter_identity(db, typevar)))
            }) {
                self.has_unresolved_type.set(true);
            }
            traversal.visit_type(db, ty);
        }
    }

    fn record_reference(&self, db: &'db dyn Db, reference: DefinitionUse<'db>) {
        let target = reference.target(db);
        self.referenced_definitions.borrow_mut().push(target);

        let Some(arguments) = reference.arguments(db, &self.env) else {
            self.inconclusive.set(true);
            return;
        };

        for (parameter, argument) in arguments {
            for (from, kind) in
                SourceParameterCollector::classify(db, &self.env, &self.source_parameters, argument)
            {
                self.edges.borrow_mut().push(FlowEdge {
                    from: FlowParameter {
                        origin: self.source_origin,
                        identity: from,
                    },
                    to: FlowParameter {
                        origin: target.origin(db),
                        identity: parameter,
                    },
                    kind,
                });
            }
        }
    }
}

/// Follow descriptor declarations before observing the callable exposed by member access.
/// The graph queues each descriptor definition once instead of invoking its __get__ method.
struct CallableMemberFlow<'a, 'db>(&'a SpecializationFlowVisitor<'db>);

impl<'db> TypeWalk<'db> for CallableMemberFlow<'_, 'db> {
    type Output = ();

    fn recursion_key(&self) -> TypeWalkKey<'db> {
        TypeWalkKey::CallableMemberFlow
    }

    fn on_cycle(
        &self,
        _traversal: &TypeWalkVisit<'_, '_, 'db, Self>,
        _ty: Type<'db>,
        _kind: CycleKind,
        _state: (),
    ) {
        self.0.inconclusive.set(true);
    }

    fn visit(
        &self,
        db: &'db dyn Db,
        walk: &TypeWalkVisit<'_, '_, 'db, Self>,
        ty: Type<'db>,
        _state: (),
    ) {
        let flow = self.0;
        let visitor = TypeVisitorContext::new(flow);
        match ty {
            Type::Union(union) => {
                for element in union.elements(db) {
                    walk.visit(db, *element);
                }
                return;
            }
            Type::Intersection(intersection) => {
                for element in intersection.positive(db) {
                    walk.visit(db, *element);
                }
                return;
            }
            Type::PropertyInstance(property) => {
                if let Some(getter) = property.getter(db) {
                    flow.visit_accessor_annotation(db, getter, None);
                }
                return;
            }
            Type::TypeAlias(_) | Type::Recursive(_) => {
                flow.inconclusive.set(true);
                return;
            }
            _ => {}
        }
        // A nongeneric descriptor class can still have a generic __get__ method. Its type
        // parameters can depend on the receiver or owner, so it must contribute a graph edge
        // even though the descriptor instance does not need a recursive identity itself.
        let descriptor_class = match ty {
            Type::NominalInstance(instance) => instance
                .stored_class(db)
                .and_then(|class| class.static_class_literal(db)),
            Type::ProtocolInstance(protocol) => protocol
                .class_origin(db)
                .and_then(|class| class.static_class_literal(db)),
            _ => None,
        };
        if let Some((class, arguments)) = descriptor_class
            && !class
                .identity_specialization(db)
                .class_member(
                    db,
                    &flow.env,
                    "__get__",
                    super::MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                )
                .place
                .is_undefined()
        {
            let reference =
                DefinitionUse::Specialized(RecursiveDefinition::DescriptorGet(class), arguments);
            flow.record_reference(db, reference);
            reference.walk_arguments(db, &visitor);
        } else {
            visitor.visit_type(db, ty.underlying_function(db));
        }
    }
}

impl<'db> TypeVisitorHooks<'db> for SpecializationFlowVisitor<'db> {
    fn program_environment(&self) -> &TypeOperations<'db> {
        &self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_recursive_type(
        &self,
        _db: &'db dyn Db,
        _visit: &TypeVisit<'_, 'db, Self>,
        _recursive: crate::types::RecursiveType<'db>,
    ) {
        self.notify_skipped_lazy_type_attributes();
    }

    fn visit_type(&self, db: &'db dyn Db, visit: &TypeVisit<'_, 'db, Self>, ty: Type<'db>) {
        if ty.is_divergent() {
            // An unfinished body can still introduce a reference back to the root. Until it is
            // resolved, the absence of that edge does not prove that specialization is bounded.
            self.has_unresolved_type.set(true);
            return;
        }

        if let Type::TypeVar(typevar) = ty {
            let identity = RecursiveDefinition::parameter_identity(db, typevar);
            if !self.source_parameters.contains(&identity) {
                // Method type parameters and captured outer parameters can depend on receiver
                // or call arguments. Without those bindings, this graph cannot prove that
                // specialization is finite.
                self.inconclusive.set(true);
            }
            return;
        }

        if let Some(reference) = RecursiveDefinition::from_type(db, ty) {
            self.record_reference(db, reference);
            if matches!(reference.target(db), RecursiveDefinition::Nominal(_)) {
                // A returned or stored instance can be inspected through ordinary attributes or
                // converted to a callable. Both projections can lead back to the current source.
                self.record_reference(db, reference.for_call());
            } else if matches!(reference.target(db), RecursiveDefinition::Constructor(_)) {
                self.record_reference(db, reference.for_member_access());
            }
            reference.walk_arguments(db, visit);
            return;
        }

        visit.walk_children_once(db, &self.visited_types);
    }

    fn visit_function_type(
        &self,
        db: &'db dyn Db,
        visit: &TypeVisit<'_, 'db, Self>,
        function: FunctionType<'db>,
    ) {
        // Only overload signatures describe calls. The implementation can carry additional
        // receiver variables that are not part of the callable's public interface.
        for signature in &function.signature(db).overloads {
            walk_signature(db, signature, visit);
        }
    }

    fn visit_bound_type_var_type(
        &self,
        _db: &'db dyn Db,
        _visit: &TypeVisit<'_, 'db, Self>,
        _bound_typevar: BoundTypeVarInstance<'db>,
    ) {
    }
}

impl<'a, 'db> SourceParameterCollector<'a, 'db> {
    fn classify(
        db: &'db dyn Db,
        env: &'a TypeOperations<'db>,
        source_parameters: &'a FxHashSet<BoundTypeVarIdentity<'db>>,
        argument: Type<'db>,
    ) -> impl Iterator<Item = (BoundTypeVarIdentity<'db>, FlowKind)> {
        let collector = Self {
            source_parameters,
            env,
            found: RefCell::default(),
            visited_types: TypeCollector::default(),
            in_nested_type: Cell::default(),
        };
        TypeVisitorContext::new(&collector).visit_type(db, argument);
        collector
            .found
            .into_inner()
            .into_iter()
            .map(|(parameter, nested)| {
                (
                    parameter,
                    if nested {
                        FlowKind::Nested
                    } else {
                        FlowKind::Direct
                    },
                )
            })
    }
}

impl<'db> TypeVisitorHooks<'db> for SourceParameterCollector<'_, 'db> {
    fn program_environment(&self) -> &TypeOperations<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_recursive_type(
        &self,
        _db: &'db dyn Db,
        _visit: &TypeVisit<'_, 'db, Self>,
        _recursive: crate::types::RecursiveType<'db>,
    ) {
        self.notify_skipped_lazy_type_attributes();
    }

    fn visit_type(&self, db: &'db dyn Db, visit: &TypeVisit<'_, 'db, Self>, ty: Type<'db>) {
        if let Type::TypeVar(typevar) = ty {
            let identity = RecursiveDefinition::parameter_identity(db, typevar);
            if self.source_parameters.contains(&identity) {
                self.found
                    .borrow_mut()
                    .entry(identity)
                    .and_modify(|nested| *nested |= self.in_nested_type.get())
                    .or_insert_with(|| self.in_nested_type.get());
            }
            return;
        }

        // Unions and intersections are normalized set operations. Reapplying the same operation
        // does not add another structural layer to a parameter.
        match ty {
            Type::Union(_union) => {
                visit.walk_children(db);
                return;
            }
            Type::Intersection(_intersection) => {
                visit.walk_children(db);
                return;
            }
            Type::SubclassOf(subclass_of) if let Some(typevar) = subclass_of.into_type_var() => {
                // Repeated metaclass projection reaches `type`; it does not
                // accumulate layers like `list[T]` or `tuple[T]`.
                visit.visit_type(db, Type::TypeVar(typevar));
                return;
            }
            _ => {}
        }

        let was_in_nested_type = self.in_nested_type.replace(true);
        if let Some(reference) = RecursiveDefinition::from_type(db, ty) {
            reference.walk_arguments(db, visit);
        } else {
            visit.walk_children_once(db, &self.visited_types);
        }
        self.in_nested_type.set(was_in_nested_type);
    }

    fn visit_bound_type_var_type(
        &self,
        _db: &'db dyn Db,
        _visit: &TypeVisit<'_, 'db, Self>,
        _bound_typevar: BoundTypeVarInstance<'db>,
    ) {
    }
}

impl<'db> TypeAliasType<'db> {
    /// Returns whether this alias can refer back to its own definition.
    pub(crate) fn is_recursive(self, db: &'db dyn Db) -> bool {
        let root = RecursiveDefinition::TypeAlias(self.unspecialized(db));
        let root_definition = root.origin(db);
        SpecializationFlowGraph::build(db, root)
            .definition_reaches(root_definition, root_definition)
    }
}

impl<'db> ProtocolInstanceType<'db> {
    fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        let (origin, _) = self.class_origin(db)?.static_class_literal(db)?;
        Some(origin.definition(db))
    }
}

/// An item that provides the identity used to detect active recursive cycles.
pub trait HasIdentity<'db> {
    type Id: PartialEq;

    /// Returns `false` if `self` and `other` cannot have the same identity.
    ///
    /// Implementations can use this to avoid constructing an expensive identity. Returning
    /// `true` does not imply that the identities match; [`HasIdentity::to_identity`] confirms it.
    fn may_share_identity(&self, _db: &'db dyn Db, _other: &Self) -> bool {
        true
    }

    /// Whether this item is a strict part of the earlier item's stored type structure.
    /// Such a visit makes finite progress even if both items have the same recursive identity.
    /// For example, inspecting `Alias[Alias[int]]` can reach its stored `Alias[int]` argument.
    fn is_structurally_smaller_than(
        &self,
        _db: &'db dyn Db,
        _env: &TypeOperations<'db>,
        _other: &Self,
    ) -> bool {
        false
    }

    /// Returns an identity that remains stable while this item is active in a [`CycleDetector`].
    fn to_identity(&self, db: &'db dyn Db, _env: &TypeOperations<'db>) -> Self::Id;
}

impl<'db> HasIdentity<'db> for Type<'db> {
    type Id = TypeIdentity<'db>;

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.may_share_type_identity(db, *other)
    }

    fn to_identity(&self, db: &'db dyn Db, _env: &TypeOperations<'db>) -> Self::Id {
        Type::to_type_identity(*self, db)
    }

    fn is_structurally_smaller_than(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        other: &Self,
    ) -> bool {
        // Callable projections store their source, not a projection of every nested argument.
        // Compare those sources to recognize progress from Wrapper[Wrapper[End]] to Wrapper[End]
        // while following their callable interfaces.
        let (current, previous) = match (*self, *other) {
            (Type::Callable(current), Type::Callable(previous))
                if let (
                    SignatureSource::Projected(current),
                    SignatureSource::Projected(previous),
                ) = (current.signature_source(db), previous.signature_source(db)) =>
            {
                (current.source(), previous.source())
            }
            _ => (*self, *other),
        };
        current != previous
            && any_over_type_including_alias_arguments(db, env, previous, |nested| {
                nested == current
            })
    }
}

pub(crate) type PairVisitor<'db, Tag, C> = CycleDetector<'db, Tag, (Type<'db>, Type<'db>), C, 1>;

impl<'db> HasIdentity<'db> for (Type<'db>, Type<'db>) {
    type Id = (TypeIdentity<'db>, TypeIdentity<'db>);

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.may_share_type_identity(db, other.0) && self.1.may_share_type_identity(db, other.1)
    }

    fn to_identity(&self, db: &'db dyn Db, _env: &TypeOperations<'db>) -> Self::Id {
        (self.0.to_type_identity(db), self.1.to_type_identity(db))
    }

    fn is_structurally_smaller_than(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        other: &Self,
    ) -> bool {
        self != other
            && (self.0 == other.0 || self.0.is_structurally_smaller_than(db, env, &other.0))
            && (self.1 == other.1 || self.1.is_structurally_smaller_than(db, env, &other.1))
    }
}

impl<'db, Context> HasIdentity<'db> for (Type<'db>, Context, Type<'db>)
where
    Context: Copy + PartialEq,
{
    type Id = (TypeIdentity<'db>, Context, TypeIdentity<'db>);

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.may_share_type_identity(db, other.0)
            && self.1 == other.1
            && self.2.may_share_type_identity(db, other.2)
    }

    fn to_identity(&self, db: &'db dyn Db, _env: &TypeOperations<'db>) -> Self::Id {
        (
            self.0.to_type_identity(db),
            self.1,
            self.2.to_type_identity(db),
        )
    }

    fn is_structurally_smaller_than(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        other: &Self,
    ) -> bool {
        self.1 == other.1
            && (self.0, self.2).is_structurally_smaller_than(db, env, &(other.0, other.2))
    }
}

/// `CycleDetector` is temporary, so callers should choose the capacity that keeps observed cycle
/// paths inline even when that makes `seen` slightly larger than an `FxIndexSet<T>`.
#[derive(Debug)]
pub struct CycleDetector<'db, Tag, T: HasIdentity<'db>, R, const INLINE_CAPACITY: usize> {
    /// The active recursion stack and the lazily-computed identity of each item.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[ActiveCycleDetectorVisit<'db, T>; INLINE_CAPACITY]>>,
    /// Identifies the active stack, restoring the previous value when a nested visit finishes.
    /// Each new visit receives a distinct value, including visits at the same depth.
    revision: Cell<u64>,
    next_revision: Cell<u64>,
    observations: Cell<u64>,

    /// Memoized results from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<T, R>>,
    cache_context: RefCell<Option<TypeOperationIdentity>>,

    fallback: R,

    _tag: PhantomData<fn() -> &'db Tag>,
}

impl<'db, Tag, T, R, const INLINE_CAPACITY: usize> CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
{
    pub(super) fn state(&self) -> RecursionGuardState<'_> {
        RecursionGuardState {
            revision: self.revision.get(),
            observations: &self.observations,
        }
    }

    fn validate_cache_owner(&self, env: &TypeOperations<'db>) {
        let context = env.identity();
        if self.cache_context.borrow().as_ref() != Some(&context) {
            *self.cache_context.borrow_mut() = Some(context);
            *self.cache.borrow_mut() = CycleDetectorCache::new();
        }
    }

    pub(crate) fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(SmallVec::new()),
            revision: Cell::new(0),
            next_revision: Cell::new(0),
            observations: Cell::new(0),
            cache: RefCell::new(CycleDetectorCache::new()),
            cache_context: RefCell::new(None),
            fallback,
            _tag: PhantomData,
        }
    }
}

impl<'db, Tag, T, R: Clone, const INLINE_CAPACITY: usize>
    CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
{
    #[inline]
    pub(crate) fn visit(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: T,
        compute: impl FnOnce() -> R,
    ) -> R {
        match self.begin_visit(db, env, item) {
            CycleDetectorVisit::Ready(result) => result,
            CycleDetectorVisit::Cycle(..) => self.fallback.clone(),
            CycleDetectorVisit::Pending(guard) => {
                let result = compute();
                self.finish_visit(env, guard, result)
            }
        }
    }

    /// Visits `item`, returning it and the previous active item in `Err` if their identities match.
    ///
    /// The caller must convert `Err((item, previous))` into an operation-specific result. An
    /// exact recursive reentry uses the detector's configured fallback and is returned as `Ok`.
    ///
    /// Completed results are reused only when `reuse_cached` accepts them. Otherwise, the visit
    /// recomputes the result using the same active recursion guards, without replacing the cached
    /// value. Results for previously uncached items are memoized as usual.
    #[inline]
    pub(super) fn try_visit(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: T,
        reuse_cached: impl FnOnce(&R) -> bool,
        compute: impl FnOnce() -> R,
    ) -> Result<R, (T, T)> {
        self.validate_cache_owner(env);
        let cached_result = self.cache.borrow().get(&item).cloned();
        let was_cached = cached_result.is_some();
        if let Some(result) = cached_result
            && reuse_cached(&result)
        {
            return Ok(result);
        }

        match self.begin_active_visit(db, env, item) {
            CycleDetectorVisit::Ready(result) => Ok(result),
            CycleDetectorVisit::Cycle(item, previous) => Err((item, previous)),
            CycleDetectorVisit::Pending(guard) => {
                let result = compute();
                if was_cached {
                    drop(guard);
                    Ok(result)
                } else {
                    Ok(self.finish_visit(env, guard, result))
                }
            }
        }
    }

    fn begin_visit(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: T,
    ) -> CycleDetectorVisit<'_, 'db, T, R, INLINE_CAPACITY> {
        self.validate_cache_owner(env);
        if let Some(result) = self.cache.borrow().get(&item) {
            return CycleDetectorVisit::Ready(result.clone());
        }

        self.begin_active_visit(db, env, item)
    }

    fn begin_active_visit(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: T,
    ) -> CycleDetectorVisit<'_, 'db, T, R, INLINE_CAPACITY> {
        self.state().observe();
        let seen = self.seen.borrow();
        if seen.iter().any(|active| active.item == item) {
            return CycleDetectorVisit::Ready(self.fallback.clone());
        }

        let candidates: SmallVec<[usize; INLINE_CAPACITY]> = seen
            .iter()
            .enumerate()
            .filter_map(|(index, active)| {
                item.may_share_identity(db, &active.item).then_some(index)
            })
            .collect();
        let index = seen.len();
        drop(seen);

        // Identity analysis may inspect a definition. Guard the exact input before starting
        // that work, and do not borrow the active stack across a query that can re-enter it.
        let next_revision = self.next_revision.get().wrapping_add(1);
        self.next_revision.set(next_revision);
        let previous_revision = self.revision.replace(next_revision);
        self.seen.borrow_mut().push(ActiveCycleDetectorVisit {
            item: item.clone(),
            identity: OnceCell::new(),
        });
        let guard = ActiveTypeVisit {
            seen: &self.seen,
            revision: &self.revision,
            previous_revision,
            item: item.clone(),
            _scope: env.enter_recursive_scope(GuardToken::new(self)),
        };
        if !candidates.is_empty() {
            self.resolve_active_identity(db, env, index);
            for previous in candidates {
                self.resolve_active_identity(db, env, previous);
                let previous = {
                    let seen = self.seen.borrow();
                    (seen[previous].identity.get() == seen[index].identity.get())
                        .then(|| seen[previous].item.clone())
                };
                if let Some(previous) = previous
                    && !item.is_structurally_smaller_than(db, env, &previous)
                {
                    return CycleDetectorVisit::Cycle(item, previous);
                }
            }
        }
        CycleDetectorVisit::Pending(guard)
    }

    fn resolve_active_identity(&self, db: &'db dyn Db, env: &TypeOperations<'db>, index: usize) {
        let item = {
            let seen = self.seen.borrow();
            seen[index]
                .identity
                .get()
                .is_none()
                .then(|| seen[index].item.clone())
        };
        if let Some(item) = item {
            let identity = item.to_identity(db, env);
            // A nested analysis can complete the same identity first.
            let _ = self.seen.borrow()[index].identity.set(identity);
        }
    }

    /// Check the cycle policy without visiting the body. Identity analysis still registers
    /// the exact input, since computing an abstract identity can itself enter a type query.
    pub(super) fn cycle_kind(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: &T,
    ) -> Option<CycleKind> {
        self.state().observe();
        if !self.has_identity_candidate(db, item) {
            return None;
        }
        match self.begin_active_visit(db, env, item.clone()) {
            CycleDetectorVisit::Ready(_) => Some(CycleKind::Exact),
            CycleDetectorVisit::Cycle(..) => Some(CycleKind::MatchingIdentity),
            CycleDetectorVisit::Pending(_guard) => None,
        }
    }

    /// A cache lookup needs no temporary guard when none of the active inputs can match it.
    fn has_identity_candidate(&self, db: &'db dyn Db, item: &T) -> bool {
        self.seen
            .borrow()
            .iter()
            .any(|active| active.item == *item || item.may_share_identity(db, &active.item))
    }

    /// Finish a [`CycleDetectorVisit::Pending`] visit and cache its result.
    fn finish_visit(
        &self,
        env: &TypeOperations<'db>,
        guard: ActiveTypeVisit<'_, 'db, T, INLINE_CAPACITY>,
        result: R,
    ) -> R {
        let item = guard.item.clone();
        drop(guard);
        self.validate_cache_owner(env);
        self.cache
            .borrow_mut()
            .insert_completed(item, result.clone());
        result
    }
}

impl<'db, Tag, T, const INLINE_CAPACITY: usize> CycleDetector<'db, Tag, T, (), INLINE_CAPACITY>
where
    T: Hash + Eq + Clone + HasIdentity<'db>,
{
    /// Find the earlier operands without consuming a body. Identity analysis still runs under
    /// the exact input's guard, just as it does when entering the comparison.
    pub(super) fn cycle_pair(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: T,
    ) -> Option<(T, T)> {
        if !self.has_identity_candidate(db, &item) {
            return None;
        }
        match self.begin_active_visit(db, env, item.clone()) {
            CycleDetectorVisit::Ready(()) => Some((item.clone(), item)),
            CycleDetectorVisit::Cycle(item, previous) => Some((item, previous)),
            CycleDetectorVisit::Pending(_guard) => None,
        }
    }

    /// Keep a comparison active and report the earlier operands when it recurs.
    /// Completed answers belong to the consuming operation, not this shared guard.
    pub(super) fn visit_active_pair<R>(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: T,
        compute: impl FnOnce(Option<(T, T)>) -> R,
    ) -> R {
        match self.begin_active_visit(db, env, item.clone()) {
            CycleDetectorVisit::Ready(()) => compute(Some((item.clone(), item))),
            CycleDetectorVisit::Cycle(item, previous) => compute(Some((item, previous))),
            CycleDetectorVisit::Pending(_guard) => compute(None),
        }
    }

    /// Run an operation that cannot provide a value when it recurses. Exact and growing cycles
    /// both return `None`; completed results are not cached.
    pub(crate) fn try_visit_active<U>(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: T,
        work: impl FnOnce() -> U,
    ) -> Option<U> {
        self.visit_active(db, env, item, |cycle| cycle.is_none().then(work))
    }

    /// Keep a visit active throughout `compute`, without caching its result.
    /// `compute` receives `None` on entry and a cycle kind when revisiting an active item.
    ///
    /// A traversal's result can depend on which enclosing types are still active. In that case,
    /// only the active recursion stack can be shared between visits.
    pub(super) fn visit_active<R>(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        item: T,
        compute: impl FnOnce(Option<CycleKind>) -> R,
    ) -> R {
        match self.begin_active_visit(db, env, item) {
            CycleDetectorVisit::Ready(()) => compute(Some(CycleKind::Exact)),
            CycleDetectorVisit::Cycle(..) => compute(Some(CycleKind::MatchingIdentity)),
            CycleDetectorVisit::Pending(_guard) => compute(None),
        }
    }
}

/// Whether a recursive operation revisited the same type or another specialization of it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CycleKind {
    Exact,
    /// The types have the same recursive identity, but their arguments can differ.
    MatchingIdentity,
    /// Another invocation of the same operation is already inspecting this type. Unlike an
    /// edge within one traversal, semantic re-entry can contribute a new projected value.
    Reentrant,
}

struct ActiveTypeVisit<'a, 'db, T: HasIdentity<'db> + PartialEq, const INLINE_CAPACITY: usize> {
    _scope: RecursiveOperationScope<'a, 'db>,
    seen: &'a RefCell<SmallVec<[ActiveCycleDetectorVisit<'db, T>; INLINE_CAPACITY]>>,
    revision: &'a Cell<u64>,
    previous_revision: u64,
    item: T,
}

impl<'db, T: HasIdentity<'db> + PartialEq, const INLINE_CAPACITY: usize> Drop
    for ActiveTypeVisit<'_, 'db, T, INLINE_CAPACITY>
{
    fn drop(&mut self) {
        self.revision.set(self.previous_revision);
        let active = self.seen.borrow_mut().pop();
        debug_assert!(
            active
                .as_ref()
                .is_some_and(|active| active.item == self.item)
        );
    }
}

struct ActiveCycleDetectorVisit<'db, T: HasIdentity<'db>> {
    item: T,
    identity: OnceCell<T::Id>,
}

impl<'db, T: fmt::Debug + HasIdentity<'db>> fmt::Debug for ActiveCycleDetectorVisit<'db, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.item.fmt(f)
    }
}

/// Result of starting a cycle-detector visit.
enum CycleDetectorVisit<'a, 'db, T: HasIdentity<'db> + PartialEq, R, const N: usize> {
    /// The item already has a completed result or hit an exact recursive edge.
    Ready(R),
    /// A different item with the same abstract identity is already pending.
    Cycle(T, T),
    /// The caller should compute the result and finish the pending visit.
    Pending(ActiveTypeVisit<'a, 'db, T, N>),
}

/// Guards recursive type transformations.
pub(crate) struct TypeTransformer<'db, Tag> {
    /// The active transformation stack and its recursive identities.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[ActiveTypeTransformation<'db>; 3]>>,

    /// Memoized transformations from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<Type<'db>, Type<'db>>>,
    cache_context: RefCell<Option<TypeOperationIdentity>>,

    _tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Default for TypeTransformer<'_, Tag> {
    fn default() -> Self {
        Self {
            seen: RefCell::default(),
            cache: RefCell::default(),
            cache_context: RefCell::new(None),
            _tag: PhantomData,
        }
    }
}

impl<'db, Tag> TypeTransformer<'db, Tag> {
    #[inline]
    fn validate_cache_owner(&self, env: &TypeOperations<'db>) {
        let context = env.identity();
        if self.cache_context.borrow().as_ref() != Some(&context) {
            *self.cache_context.borrow_mut() = Some(context);
            *self.cache.borrow_mut() = CycleDetectorCache::new();
        }
    }

    pub(crate) fn visit_type(
        &self,
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        ty: Type<'db>,
        compute: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        self.visit_with_identity(env, ty, || ty.to_type_identity(db), compute)
    }

    /// Rewrite stored type data without inspecting the definition it refers to.
    pub(crate) fn visit_stored_type(
        &self,
        env: &TypeOperations<'db>,
        ty: Type<'db>,
        compute: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        self.visit_with_identity(env, ty, || TypeIdentity::Other(ty), compute)
    }

    fn visit_with_identity(
        &self,
        env: &TypeOperations<'db>,
        ty: Type<'db>,
        identity: impl FnOnce() -> TypeIdentity<'db>,
        compute: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        self.validate_cache_owner(env);
        if let Some(result) = self.cache.borrow().get(&ty) {
            return *result;
        }
        if self.seen.borrow().iter().any(|active| active.ty == ty) {
            return ty;
        }

        // Guard the exact input before identity analysis, which can infer another definition.
        let scope = env.enter_recursive_scope(GuardToken::new(self));
        let index = self.seen.borrow().len();
        self.seen
            .borrow_mut()
            .push(ActiveTypeTransformation { ty, identity: None });
        let guard = TypeTransformationGuard { seen: &self.seen };
        let identity = identity();
        {
            let mut seen = self.seen.borrow_mut();
            if seen[..index]
                .iter()
                .any(|active| active.identity == Some(identity))
            {
                return ty;
            }
            seen[index].identity = Some(identity);
        }
        let result = compute();
        drop(guard);
        drop(scope);
        self.validate_cache_owner(env);
        self.cache.borrow_mut().insert_completed(ty, result);
        result
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveTypeTransformation<'db> {
    ty: Type<'db>,
    identity: Option<TypeIdentity<'db>>,
}

struct TypeTransformationGuard<'a, 'db> {
    seen: &'a RefCell<SmallVec<[ActiveTypeTransformation<'db>; 3]>>,
}

impl Drop for TypeTransformationGuard<'_, '_> {
    fn drop(&mut self) {
        self.seen.borrow_mut().pop();
    }
}

impl<'db, Tag, T, R: Default, const INLINE_CAPACITY: usize> Default
    for CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
{
    fn default() -> Self {
        CycleDetector::new(R::default())
    }
}

/// The memoized results for a [`CycleDetector`].
///
/// Most populated cycle-detector caches contain at most two results. Keep those results inline,
/// but spill on the third distinct result so lookups in wider caches remain hashed.
#[derive(Debug, Default)]
pub(super) enum CycleDetectorCache<T, R> {
    #[default]
    Empty,
    One((T, R)),
    Two([(T, R); 2]),
    Spilled(FxHashMap<T, R>),
}

impl<T, R> CycleDetectorCache<T, R> {
    pub(super) const fn new() -> Self {
        Self::Empty
    }

    pub(super) fn get(&self, item: &T) -> Option<&R>
    where
        T: Hash + Eq,
    {
        match self {
            Self::Empty => None,
            Self::One((cached_item, result)) => (cached_item == item).then_some(result),
            Self::Two(entries) => entries
                .iter()
                .find_map(|(cached_item, result)| (cached_item == item).then_some(result)),
            Self::Spilled(cache) => cache.get(item),
        }
    }

    /// Store a result, replacing an answer that no longer applies in the current context.
    pub(super) fn insert(&mut self, item: T, result: R)
    where
        T: Hash + Eq,
    {
        let entries = match self {
            Self::One(entry) => std::slice::from_mut(entry),
            Self::Two(entries) => entries.as_mut_slice(),
            Self::Empty | Self::Spilled(_) => &mut [],
        };
        if let Some((_, cached)) = entries.iter_mut().find(|(key, _)| *key == item) {
            *cached = result;
        } else {
            self.insert_new(item, result);
        }
    }

    /// Inserts a completed item after the caller has checked that `item` is not already cached.
    fn insert_completed(&mut self, item: T, result: R)
    where
        T: Hash + Eq,
    {
        debug_assert!(self.get(&item).is_none());
        self.insert_new(item, result);
    }

    fn insert_new(&mut self, item: T, result: R)
    where
        T: Hash + Eq,
    {
        let entry = (item, result);
        *self = match mem::replace(self, Self::Empty) {
            Self::Empty => Self::One(entry),
            Self::One(first) => Self::Two([first, entry]),
            Self::Two(entries) => Self::spill(entries, entry),
            Self::Spilled(mut cache) => {
                cache.insert(entry.0, entry.1);
                Self::Spilled(cache)
            }
        };
    }

    #[cold]
    fn spill(entries: [(T, R); 2], third: (T, R)) -> Self
    where
        T: Hash + Eq,
    {
        Self::Spilled(entries.into_iter().chain([third]).collect())
    }

    #[cfg(test)]
    const fn is_spilled(&self) -> bool {
        matches!(self, Self::Spilled(_))
    }
}

/// Recursion detection without memoization.
///
/// This is useful when a recursive relation needs a coinductive-style "we're already proving this
/// goal, assume it for now" step, but completed results are not safe to reuse for future visits to
/// the same abstract key.
#[derive(Debug)]
pub(crate) struct ActiveRecursionDetector<T> {
    seen: RefCell<SmallSet<T, 2>>,
    /// Completed nested visits restore this value so they do not invalidate cached queries.
    revision: Cell<u64>,
    next_revision: Cell<u64>,
    observations: Cell<u64>,
}

impl<T> Default for ActiveRecursionDetector<T> {
    fn default() -> Self {
        Self {
            seen: RefCell::default(),
            revision: Cell::new(0),
            next_revision: Cell::new(0),
            observations: Cell::new(0),
        }
    }
}

impl<T: Hash + Eq + Clone> ActiveRecursionDetector<T> {
    pub(super) fn state(&self) -> RecursionGuardState<'_> {
        RecursionGuardState {
            revision: self.revision.get(),
            observations: &self.observations,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.state().observe();
        self.seen.borrow().len()
    }

    pub(super) fn contains(&self, item: &T) -> bool {
        self.state().observe();
        self.seen.borrow().contains(item)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.state().observe();
        self.seen.borrow().is_empty()
    }

    pub(crate) fn visit<R>(
        &self,
        env: &TypeOperations<'_>,
        item: &T,
        on_cycle: impl FnOnce() -> R,
        func: impl FnOnce() -> R,
    ) -> R {
        self.state().observe();
        if !self.seen.borrow_mut().insert(item.clone()) {
            return on_cycle();
        }

        let next_revision = self.next_revision.get().wrapping_add(1);
        self.next_revision.set(next_revision);
        let previous_revision = self.revision.replace(next_revision);

        // Keep the active-recursion state scoped even if `func` unwinds. In some cases, we catch
        // panics and continue handling later work on the same thread.
        let _guard = ActiveRecursionGuard {
            seen: &self.seen,
            revision: &self.revision,
            previous_revision,
            item,
            _scope: env.enter_recursive_scope(GuardToken::new(self)),
        };

        func()
    }
}

impl ActiveRecursionDetector<()> {
    /// A guard with one possible key has only two observable states.
    pub(super) fn presence_state(&self) -> RecursionGuardState<'_> {
        RecursionGuardState {
            revision: u64::from(!self.seen.borrow().is_empty()),
            observations: &self.observations,
        }
    }
}

/// Reject a result if computing it revisits any active key in this operation.
///
/// Returning a fallback only for the repeated call can leave enclosing computations with a
/// partially expanded result. This guard propagates incompleteness through every enclosing visit,
/// including mutually recursive keys. Later independent visits can still complete normally.
pub(super) struct AcyclicRecursionDetector<T> {
    active: ActiveRecursionDetector<T>,
    cycles: Cell<u64>,
}

impl<T> Default for AcyclicRecursionDetector<T> {
    fn default() -> Self {
        Self {
            active: ActiveRecursionDetector::default(),
            cycles: Cell::new(0),
        }
    }
}

impl<T: Hash + Eq + Clone> AcyclicRecursionDetector<T> {
    pub(super) fn states(&self) -> [RecursionGuardState<'_>; 2] {
        // Restoring the active set must not erase an enclosing computation's incompleteness.
        [
            self.active.state(),
            RecursionGuardState {
                revision: self.cycles.get(),
                observations: &self.active.observations,
            },
        ]
    }

    pub(super) fn visit<R>(
        &self,
        env: &TypeOperations<'_>,
        item: &T,
        compute: impl FnOnce() -> R,
    ) -> Option<R> {
        let cycles = self.cycles.get();
        self.active.visit(
            env,
            item,
            || {
                self.cycles.set(self.cycles.get().wrapping_add(1));
                None
            },
            || {
                let result = compute();
                (self.cycles.get() == cycles).then_some(result)
            },
        )
    }
}

struct ActiveRecursionGuard<'a, 'db, T: Hash + Eq> {
    _scope: RecursiveOperationScope<'a, 'db>,
    seen: &'a RefCell<SmallSet<T, 2>>,
    revision: &'a Cell<u64>,
    previous_revision: u64,
    item: &'a T,
}

impl<T: Hash + Eq> Drop for ActiveRecursionGuard<'_, '_, T> {
    fn drop(&mut self) {
        self.revision.set(self.previous_revision);
        self.seen.borrow_mut().remove(self.item);
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::{
        AcyclicRecursionDetector, CycleDetector, CycleDetectorVisit, CycleKind, Db, FlowEdge,
        FlowKind, HasIdentity, RecursiveDefinition, SpecializationFlowGraph, TypeIdentity,
        TypeTransformer,
    };
    use crate::TypeOperations;
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::{KnownInstanceType, Type, TypeAliasType};
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use std::cell::Cell;
    use std::hash::{Hash, Hasher};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use ty_python_core::ProgramFile;

    struct TestVisit;

    #[test]
    fn incomplete_results_invalidate_enclosing_visits() {
        let db = setup_db();
        let env = db.program_environment();
        let detector = AcyclicRecursionDetector::default();

        let incomplete = detector.visit(&env, &1, || {
            detector
                .visit(&env, &2, || detector.visit(&env, &1, || 10).unwrap_or(20))
                .unwrap_or(30)
        });
        assert_eq!(incomplete, None);
        assert_eq!(detector.visit(&env, &1, || 40), Some(40));
    }

    #[test]
    fn guards_transformations_before_resolving_identity() {
        let db = setup_db();
        let env = db.program_environment();
        let transformer = TypeTransformer::<TestVisit>::default();
        let ty = Type::unknown();
        let result = transformer.visit_with_identity(
            &env,
            ty,
            || {
                assert_eq!(transformer.visit_stored_type(&env, ty, || Type::Never), ty);
                TypeIdentity::Other(ty)
            },
            || Type::Never,
        );
        assert_eq!(result, Type::Never);
    }

    #[test]
    fn transformation_visits_are_released_when_unwinding() {
        let db = setup_db();
        let env = db.program_environment();
        for cancel_during_identity in [false, true] {
            let transformer = TypeTransformer::<TestVisit>::default();
            let ty = Type::unknown();
            let result = catch_unwind(AssertUnwindSafe(|| {
                transformer.visit_with_identity(
                    &env,
                    ty,
                    || {
                        assert!(!cancel_during_identity, "cancel identity inference");
                        TypeIdentity::Other(ty)
                    },
                    || panic!("cancel transformation"),
                );
            }));
            assert!(result.is_err());
            assert_eq!(
                transformer.visit_stored_type(&env, ty, || Type::Never),
                Type::Never
            );
        }
    }

    type Detector<'db> = CycleDetector<'db, TestVisit, u8, u8, 1>;

    impl<'db> HasIdentity<'db> for u8 {
        type Id = Self;

        fn to_identity(&self, _db: &'db dyn Db, _env: &TypeOperations<'db>) -> Self::Id {
            *self
        }
    }

    #[derive(Clone)]
    struct CountingIdentityItem<'a> {
        value: u8,
        identity_calls: &'a Cell<usize>,
    }

    impl<'a> CountingIdentityItem<'a> {
        const fn new(value: u8, identity_calls: &'a Cell<usize>) -> Self {
            Self {
                value,
                identity_calls,
            }
        }
    }

    impl PartialEq for CountingIdentityItem<'_> {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }

    impl Eq for CountingIdentityItem<'_> {}

    impl Hash for CountingIdentityItem<'_> {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.value.hash(state);
        }
    }

    impl<'db> HasIdentity<'db> for CountingIdentityItem<'_> {
        type Id = u8;

        fn may_share_identity(&self, _db: &'db dyn Db, other: &Self) -> bool {
            self.value % 2 == other.value % 2
        }

        fn to_identity(&self, _db: &'db dyn Db, _env: &TypeOperations<'db>) -> Self::Id {
            self.identity_calls.set(self.identity_calls.get() + 1);
            self.value
        }
    }

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    struct ConstantIdentityItem(u8);

    impl<'db> HasIdentity<'db> for ConstantIdentityItem {
        type Id = ();

        fn to_identity(&self, _db: &'db dyn Db, _env: &TypeOperations<'db>) -> Self::Id {}
    }

    fn global_instance_type<'db>(
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        name: &str,
    ) -> Type<'db> {
        let file = system_path_to_file(db, "/src/a.py").unwrap();
        let file = ProgramFile::new(db, file, env.program(db));
        global_symbol(db, file, name)
            .place
            .expect_type()
            .to_instance_approximation(db, env)
            .unwrap()
    }

    fn global_type_alias<'db>(
        db: &'db dyn Db,
        env: &TypeOperations<'db>,
        name: &str,
    ) -> TypeAliasType<'db> {
        let file = system_path_to_file(db, "/src/a.py").unwrap();
        let file = ProgramFile::new(db, file, env.program(db));
        let Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) =
            global_symbol(db, file, name).place.expect_type()
        else {
            panic!("expected `{name}` to be a type alias");
        };
        alias
    }

    #[test]
    fn lambda_identities_preserve_runtime_classes() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
class Node[T]:
    def __init__(self, value: T):
        self.callback = lambda: (value, Node([value]).callback)

callback = Node(0).callback
"#,
        )?;
        let env = db.program_environment();
        let file = system_path_to_file(&db, "/src/a.py")?;
        let file = ProgramFile::new(&db, file, env.program(&db));
        let Type::Callable(callback) = global_symbol(&db, file, "callback").place.expect_type()
        else {
            panic!("expected a callable");
        };
        let function = Type::Callable(callback);
        let regular = Type::Callable(callback.into_regular(&db));
        assert_matches!(
            function.to_type_identity(&db),
            TypeIdentity::GrowingLambda(..)
        );
        assert!(!function.may_share_type_identity(&db, regular));
        assert_ne!(
            function.to_type_identity(&db),
            regular.to_type_identity(&db)
        );
        Ok(())
    }

    #[test]
    fn combines_flows_from_multiple_recursive_references() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
type Alternating[X, Y] = tuple[
    Alternating[Y, None],
    Alternating[None, list[X]],
]
"#,
        )
        .unwrap();
        let env = db.program_environment();

        assert!(matches!(
            Type::TypeAlias(global_type_alias(&db, &env, "Alternating")).recursive_identity(&db),
            Some(TypeIdentity::GrowingTypeAlias(_))
        ));
    }

    #[test]
    fn classifies_flow_graph_cycles() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
type One[T] = T
type Two[X, Y] = tuple[X, Y]
type Helper[U] = U
"#,
        )
        .unwrap();
        let env = db.program_environment();
        let one =
            RecursiveDefinition::TypeAlias(global_type_alias(&db, &env, "One").unspecialized(&db));
        let two =
            RecursiveDefinition::TypeAlias(global_type_alias(&db, &env, "Two").unspecialized(&db));
        let helper = RecursiveDefinition::TypeAlias(
            global_type_alias(&db, &env, "Helper").unspecialized(&db),
        );
        let mut one_parameters = one.parameters(&db);
        let Some(one_t) = one_parameters.next() else {
            panic!("expected one parameter");
        };
        let mut two_parameters = two.parameters(&db);
        let (Some(two_x), Some(two_y)) = (two_parameters.next(), two_parameters.next()) else {
            panic!("expected two parameters");
        };
        let mut helper_parameters = helper.parameters(&db);
        let Some(helper_u) = helper_parameters.next() else {
            panic!("expected one helper parameter");
        };

        for (root, edges, expected) in [
            (
                one,
                vec![FlowEdge {
                    from: one_t,
                    to: one_t,
                    kind: FlowKind::Direct,
                }],
                false,
            ),
            (
                one,
                vec![FlowEdge {
                    from: one_t,
                    to: one_t,
                    kind: FlowKind::Nested,
                }],
                true,
            ),
            (
                two,
                vec![
                    FlowEdge {
                        from: two_x,
                        to: two_y,
                        kind: FlowKind::Direct,
                    },
                    FlowEdge {
                        from: two_y,
                        to: two_x,
                        kind: FlowKind::Direct,
                    },
                ],
                false,
            ),
            (
                two,
                vec![FlowEdge {
                    from: two_y,
                    to: two_x,
                    kind: FlowKind::Nested,
                }],
                false,
            ),
            (
                two,
                vec![
                    FlowEdge {
                        from: two_y,
                        to: two_x,
                        kind: FlowKind::Nested,
                    },
                    FlowEdge {
                        from: two_x,
                        to: two_y,
                        kind: FlowKind::Direct,
                    },
                ],
                true,
            ),
            (
                one,
                vec![FlowEdge {
                    from: one_t,
                    to: helper_u,
                    kind: FlowKind::Nested,
                }],
                false,
            ),
            (
                one,
                vec![
                    FlowEdge {
                        from: one_t,
                        to: helper_u,
                        kind: FlowKind::Nested,
                    },
                    FlowEdge {
                        from: helper_u,
                        to: one_t,
                        kind: FlowKind::Direct,
                    },
                ],
                true,
            ),
        ] {
            let graph = SpecializationFlowGraph {
                edges: edges.into_iter().collect(),
                ..SpecializationFlowGraph::default()
            };
            assert_eq!(
                graph.root_may_have_unbounded_specialization(&db, root),
                expected,
            );
        }
    }

    #[test]
    fn scopes_inconclusive_parameter_flows_to_recursive_paths() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from typing import Protocol

class Outer[T](Protocol):
    type Inner = T
    value: Inner

class RecursiveOuter[T](Protocol):
    type Inner = tuple[T, RecursiveOuter[list[T]]]
    value: Inner
"#,
        )
        .unwrap();
        let env = db.program_environment();

        assert!(
            global_instance_type(&db, &env, "Outer")
                .recursive_identity(&db)
                .is_none()
        );
        assert!(matches!(
            global_instance_type(&db, &env, "RecursiveOuter").recursive_identity(&db),
            Some(TypeIdentity::GrowingProtocol(_))
        ));
    }

    #[test]
    fn classifies_recursive_parameter_flows() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
type Stable[T] = tuple[T, Stable[T]]
type Growing[T] = tuple[T, Growing[list[T]]]
type Swap[X, Y] = tuple[X, Swap[Y, X]]
type ShiftReset[X, Y] = tuple[X, ShiftReset[list[Y], None]]
type ShiftCycle[X, Y] = tuple[X, ShiftCycle[list[Y], X]]

type ResetOuter[T] = tuple[T, ResetHelper[list[T]]]
type ResetHelper[U] = tuple[U, ResetOuter[int]]

type TransitiveOuter[T] = tuple[T, TransitiveHelper[list[T]]]
type TransitiveHelper[U] = tuple[U, TransitiveOuter[U]]

type PeriodicOuter[X, Y] = tuple[X, PeriodicHelper[X, Y]]
type PeriodicHelper[X, Y] = tuple[X, PeriodicHelper[Y, X]]

type Saturating[T] = tuple[T, Saturating[T | int]]
"#,
        )
        .unwrap();
        let env = db.program_environment();

        for (name, expected) in [
            ("Stable", false),
            ("Growing", true),
            ("Swap", false),
            ("ShiftReset", false),
            ("ShiftCycle", true),
            ("ResetOuter", false),
            ("ResetHelper", false),
            ("TransitiveOuter", true),
            ("TransitiveHelper", true),
            ("PeriodicOuter", false),
            ("PeriodicHelper", false),
            ("Saturating", false),
        ] {
            let alias = RecursiveDefinition::TypeAlias(
                global_type_alias(&db, &env, name).unspecialized(&db),
            );
            assert_eq!(
                alias.may_have_unbounded_specialization(&db),
                expected,
                "unexpected result for {name}",
            );
        }
    }

    #[test]
    fn property_receiver_does_not_make_protocol_recursive() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            r#"
from __future__ import annotations

from typing import Protocol

class GenericProperty[T](Protocol):
    @property
    def value(self) -> T: ...

class RecursiveProperty[T](Protocol):
    @property
    def child(self) -> RecursiveProperty[list[T]]: ...

class RecursivePropertySetter[T](Protocol):
    @property
    def child(self) -> int: ...

    @child.setter
    def child(self, value: RecursivePropertySetter[list[T]]) -> None: ...
"#,
        )
        .unwrap();

        let env = db.program_environment();
        assert_eq!(
            global_instance_type(&db, &env, "GenericProperty").recursive_identity(&db),
            None
        );
        assert_matches!(
            global_instance_type(&db, &env, "RecursiveProperty").recursive_identity(&db),
            Some(TypeIdentity::GrowingProtocol(_))
        );
        assert_matches!(
            global_instance_type(&db, &env, "RecursivePropertySetter").recursive_identity(&db),
            Some(TypeIdentity::GrowingProtocol(_))
        );
    }

    #[test]
    fn caches_results_and_spills_after_two_entries() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(detector.visit(db, &env, 1, || 10), 10);
        assert_eq!(detector.visit(db, &env, 1, || 40), 10);
        assert_eq!(detector.visit(db, &env, 2, || 20), 20);
        assert!(!detector.cache.borrow().is_spilled());
        assert_eq!(detector.visit(db, &env, 3, || 30), 30);
        assert!(detector.cache.borrow().is_spilled());

        assert_eq!(detector.visit(db, &env, 2, || 40), 20);
        assert_eq!(detector.visit(db, &env, 3, || 40), 30);
    }

    #[test]
    fn nested_visit_short_circuits_on_cycle() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(
            detector.visit(db, &env, 1, || detector.visit(db, &env, 1, || 20) + 10),
            10
        );
    }

    #[test]
    fn active_visits_distinguish_cycles_without_caching_results() {
        let db = setup_db();
        let env = db.program_environment();
        let detector = CycleDetector::<'_, TestVisit, ConstantIdentityItem, (), 1>::new(());

        detector.visit_active(&db, &env, ConstantIdentityItem(1), |cycle| {
            assert_eq!(cycle, None);
            assert_eq!(
                detector.visit_active(&db, &env, ConstantIdentityItem(1), |cycle| cycle),
                Some(CycleKind::Exact),
            );
            assert_eq!(
                detector.visit_active(&db, &env, ConstantIdentityItem(2), |cycle| cycle),
                Some(CycleKind::MatchingIdentity),
            );
        });

        assert_eq!(
            detector.visit_active(&db, &env, ConstantIdentityItem(2), |cycle| cycle),
            None,
        );
    }

    #[test]
    fn active_visits_are_released_when_unwinding() {
        let db = setup_db();
        let env = db.program_environment();
        let detector = CycleDetector::<'_, TestVisit, u8, (), 1>::new(());
        let result = catch_unwind(AssertUnwindSafe(|| {
            detector.visit_active(&db, &env, 1, |_| panic!("stop the traversal"));
        }));
        assert!(result.is_err());
        assert!(detector.visit_active(&db, &env, 1, |cycle| cycle.is_none()));
    }

    #[test]
    fn cached_visits_are_released_when_unwinding() {
        let db = setup_db();
        let env = db.program_environment();
        for recompute in [false, true] {
            let detector = Detector::new(0);
            if recompute {
                detector.visit(&db, &env, 1, || 10);
            }
            let result = catch_unwind(AssertUnwindSafe(|| {
                if recompute {
                    let _ =
                        detector.try_visit(&db, &env, 1, |_| false, || panic!("cancel inference"));
                } else {
                    detector.visit(&db, &env, 1, || panic!("cancel inference"));
                }
            }));
            assert!(result.is_err());
            assert_eq!(detector.try_visit(&db, &env, 1, |_| false, || 20), Ok(20));
        }
    }

    #[test]
    fn identity_visits_are_released_when_unwinding() {
        #[derive(Clone, Copy, Eq, Hash, PartialEq)]
        struct Item(u8);

        impl<'db> HasIdentity<'db> for Item {
            type Id = ();

            fn to_identity(&self, _db: &'db dyn Db, _env: &TypeOperations<'db>) {
                assert!(self.0 != 2, "cancel identity analysis");
            }
        }

        let db = setup_db();
        let env = db.program_environment();
        let detector = CycleDetector::<'_, TestVisit, Item, u8, 1>::new(0);
        assert_eq!(
            detector.visit(&db, &env, Item(1), || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    detector.visit(&db, &env, Item(2), || 20);
                }));
                assert!(result.is_err());
                10
            }),
            10
        );
        assert_eq!(detector.visit(&db, &env, Item(3), || 30), 30);
    }

    #[test]
    fn selectively_reuses_cached_results() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(detector.try_visit(db, &env, 1, |_| true, || 10), Ok(10));
        assert_eq!(
            detector.try_visit(db, &env, 1, |&result| result == 10, || 20),
            Ok(10)
        );
        assert_eq!(
            detector.try_visit(
                db,
                &env,
                1,
                |&result| result == 20,
                || {
                    assert_eq!(detector.try_visit(db, &env, 1, |_| false, || 30), Ok(0));
                    detector.visit(db, &env, 1, || 30) + 10
                }
            ),
            Ok(20)
        );
        assert_eq!(detector.visit(db, &env, 1, || 30), 10);
    }

    #[test]
    fn recomputed_visits_share_exact_recursion_guards() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(
            detector.try_visit(
                db,
                &env,
                1,
                |_| false,
                || detector.visit(db, &env, 1, || 20) + 10
            ),
            Ok(10)
        );
        assert_eq!(
            detector.visit(db, &env, 2, || {
                assert_eq!(detector.try_visit(db, &env, 2, |_| false, || 20), Ok(0));
                10
            }),
            10
        );
    }

    #[test]
    fn recomputed_visits_share_abstract_identity_guards() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let detector = CycleDetector::<TestVisit, ConstantIdentityItem, u8, 1>::new(0);

        assert_eq!(
            detector.try_visit(
                db,
                &env,
                ConstantIdentityItem(1),
                |_| false,
                || {
                    assert_eq!(
                        detector.try_visit(db, &env, ConstantIdentityItem(2), |_| true, || 20),
                        Err((ConstantIdentityItem(2), ConstantIdentityItem(1)))
                    );
                    10
                }
            ),
            Ok(10)
        );
        assert_eq!(
            detector.try_visit(
                db,
                &env,
                ConstantIdentityItem(3),
                |_| true,
                || {
                    assert_eq!(
                        detector.try_visit(db, &env, ConstantIdentityItem(4), |_| false, || 20),
                        Err((ConstantIdentityItem(4), ConstantIdentityItem(3)))
                    );
                    10
                }
            ),
            Ok(10)
        );
    }

    #[test]
    fn computes_each_active_identity_once() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(
                db,
                &env,
                CountingIdentityItem::new(1, &identity_calls),
                || {
                    detector.visit(
                        db,
                        &env,
                        CountingIdentityItem::new(3, &identity_calls),
                        || 1,
                    )
                }
            ),
            1
        );
        assert_eq!(identity_calls.get(), 2);
    }

    #[test]
    fn skips_identity_for_distinct_candidates() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(
                db,
                &env,
                CountingIdentityItem::new(1, &identity_calls),
                || {
                    detector.visit(
                        db,
                        &env,
                        CountingIdentityItem::new(2, &identity_calls),
                        || 1,
                    )
                }
            ),
            1
        );
        assert_eq!(identity_calls.get(), 0);
    }

    #[test]
    fn skips_identity_without_a_distinct_active_item() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(
                db,
                &env,
                CountingIdentityItem::new(1, &identity_calls),
                || 1
            ),
            1
        );
        assert_eq!(
            detector.visit(
                db,
                &env,
                CountingIdentityItem::new(1, &identity_calls),
                || 2
            ),
            1
        );
        assert_eq!(identity_calls.get(), 0);
    }

    #[test]
    fn different_items_with_same_identity_form_cycle() {
        let db = setup_db();
        let env = db.program_environment();
        let db = &db;
        let detector = CycleDetector::<TestVisit, ConstantIdentityItem, u8, 1>::new(0);

        let CycleDetectorVisit::Pending(pending) =
            detector.begin_visit(db, &env, ConstantIdentityItem(1))
        else {
            panic!("the first identity should be pending");
        };
        let CycleDetectorVisit::Cycle(item, previous) =
            detector.begin_visit(db, &env, ConstantIdentityItem(2))
        else {
            panic!("a different item with the same identity should form a cycle");
        };
        assert_eq!(item.0, 2);
        assert_eq!(previous.0, 1);
        detector.finish_visit(&env, pending, 1);

        let CycleDetectorVisit::Ready(seen) =
            detector.begin_visit(db, &env, ConstantIdentityItem(1))
        else {
            panic!("the first identity should be ready after the pending visit is finished");
        };
        assert_eq!(seen, 1);
        let CycleDetectorVisit::Pending(pending) =
            detector.begin_visit(db, &env, ConstantIdentityItem(2))
        else {
            panic!("the second identity should be pending after the first is finished");
        };
        detector.finish_visit(&env, pending, 2);
        let CycleDetectorVisit::Ready(seen) =
            detector.begin_visit(db, &env, ConstantIdentityItem(2))
        else {
            panic!("the second identity should be ready after the pending visit is finished");
        };
        assert_eq!(seen, 2);
    }
}
