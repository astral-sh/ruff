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

use crate::place::Place;
use crate::types::constructor::ConstructorMembers;
use crate::types::function::FunctionLiteral;
use crate::types::generics::{GenericContext, Specialization, walk_specialization_types};
use crate::types::known_instance::MethodWrapperKind;
use crate::types::relation::RelationObservation;
use crate::types::typevar::{TypeVarInstance, TypeVarSet};
use crate::types::visitor::{
    TypeCollector, TypeKind, TypeVisitor, walk_non_atomic_type, walk_type_with_recursion_guard,
};
use crate::types::{
    BoundTypeVarIdentity, BoundTypeVarInstance, CallableTypes, ClassType, DescriptorDispatch,
    DescriptorDispatches, DescriptorOrigin, GenericAlias, KnownBoundMethodType, KnownInstanceType,
    LiteralValueTypeKind, MemberLookupPolicy, ProtocolInstanceType, RecursiveType,
    StaticClassLiteral, SubclassOfInner, SubclassOfType, Type, TypeAliasType,
    TypeVarBoundOrConstraints, TypedDictType,
};
use crate::{Db, Program, ProgramEnvironment};

/// The type identity used for recursive checks/transformations.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub enum TypeIdentity<'db> {
    FunctionLiteral(FunctionLiteral<'db>),
    NewTypeInstance(Definition<'db>),
    GrowingProtocol(Definition<'db>),
    GrowingTypeAlias(Definition<'db>),
    GrowingTypedDict(Definition<'db>),
    GrowingRecursive(Definition<'db>),
    Other(Type<'db>),
}

impl<'db> Type<'db> {
    pub(crate) fn to_type_identity(self, db: &'db dyn Db) -> TypeIdentity<'db> {
        self.recursive_identity(db)
            .unwrap_or(TypeIdentity::Other(self))
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
            (Type::ProtocolInstance(a), Type::ProtocolInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            (Type::TypeAlias(a), Type::TypeAlias(b)) => a.definition(db) == b.definition(db),
            (Type::TypedDict(a), Type::TypedDict(b)) => a.definition(db) == b.definition(db),
            (Type::Recursive(a), Type::Recursive(b)) => a.definition(db) == b.definition(db),
            _ => false,
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
            // Recursive aliases, protocols, and TypedDicts whose specialization can keep changing
            // (e.g. `type Growing[T] = T | Growing[list[T]]`) are collapsed to their definition so
            // that visits stop even though no exact type repeats. Recursion that revisits one
            // exact specialization (e.g. `type RecursiveT = int | tuple[RecursiveT, ...]`) needs
            // no definition-level identity: the detectors stop on the repeated type itself.
            Type::TypeAlias(_)
            | Type::ProtocolInstance(_)
            | Type::TypedDict(_)
            | Type::Recursive(_) => {
                let target = RecursiveDefinition::from_type(db, self)?.target;
                if !target.may_have_unbounded_specialization(db) {
                    return None;
                }
                let definition = target.definition(db);
                Some(match target {
                    RecursiveDefinition::TypeAlias(_) => TypeIdentity::GrowingTypeAlias(definition),
                    RecursiveDefinition::Protocol(_) => TypeIdentity::GrowingProtocol(definition),
                    RecursiveDefinition::TypedDict(_) => TypeIdentity::GrowingTypedDict(definition),
                    RecursiveDefinition::Structural(_) => {
                        TypeIdentity::GrowingRecursive(definition)
                    }
                    RecursiveDefinition::Callable(_, _) => return None,
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
            Some(TypeIdentity::GrowingRecursive(_))
        )
    }
}

/// A definition whose formal parameters can flow through recursive type references.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum RecursiveDefinition<'db> {
    TypeAlias(TypeAliasType<'db>),
    Protocol(StaticClassLiteral<'db>),
    TypedDict(StaticClassLiteral<'db>),
    Structural(RecursiveType<'db>),
    Callable(CallableDefinition<'db>, CallableExpansion),
}

/// Selects lookup semantics for dependency discovery. Direct calls preserve class-object receivers
/// that callable upcasting can normalize, which can select different descriptor overloads.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue,
)]
pub(super) enum CallableExpansion {
    Bindings,
    #[default]
    Upcast,
}

/// Constructor expansion and `__call__` expansion have different dependencies, even on the same class.
/// Exact class objects and subclass types can also select different descriptor overloads.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
enum CallableDefinition<'db> {
    Constructor(StaticClassLiteral<'db>),
    SubclassConstructor(StaticClassLiteral<'db>),
    Instance(StaticClassLiteral<'db>),
}

impl<'db> CallableDefinition<'db> {
    fn from_type(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        mode: CallableExpansion,
    ) -> Option<DefinitionUse<'db>> {
        let (class, is_constructor) = match ty {
            Type::ClassLiteral(class) => (class.identity_specialization(db), true),
            Type::GenericAlias(alias) => (super::ClassType::Generic(alias), true),
            Type::SubclassOf(subclass) => match subclass.subclass_of() {
                SubclassOfInner::Class(class) => (class, true),
                SubclassOfInner::Protocol(protocol) => (*protocol.class_origin(db)?, true),
                _ => return None,
            },
            Type::NominalInstance(instance) => (instance.class(db, env), false),
            Type::ProtocolInstance(protocol) => (*protocol.class_origin(db)?, false),
            _ => return None,
        };
        let (origin, specialization) = class.static_class_literal(db)?;
        let definition = if matches!(ty, Type::SubclassOf(_)) {
            Self::SubclassConstructor(origin)
        } else if is_constructor {
            Self::Constructor(origin)
        } else {
            Self::Instance(origin)
        };
        Some(DefinitionUse {
            target: RecursiveDefinition::Callable(definition, mode),
            specialization,
        })
    }

    fn origin(self) -> StaticClassLiteral<'db> {
        match self {
            Self::Constructor(origin)
            | Self::SubclassConstructor(origin)
            | Self::Instance(origin) => origin,
        }
    }

    fn identity_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        let class = self.origin().identity_specialization(db);
        match self {
            Self::Constructor(_) => Type::from(class),
            Self::SubclassConstructor(_) => SubclassOfType::from(db, env, class),
            Self::Instance(_) => Type::instance(db, env, class),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum FlowKind {
    /// The source parameter is passed through operations that cannot accumulate structure.
    Direct,
    /// The source parameter occurs inside a type structure that can accumulate.
    Nested,
}

/// Formal parameters are identified by their [`BoundTypeVarIdentity`], which is unique across
/// definitions, so parameter identities can serve directly as the flow graph's nodes.
/// e.g.
///
/// definition:
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
    from: BoundTypeVarIdentity<'db>,
    to: BoundTypeVarIdentity<'db>,
    kind: FlowKind,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct DefinitionUse<'db> {
    target: RecursiveDefinition<'db>,
    specialization: Option<Specialization<'db>>,
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
struct SpecializationFlowGraph<'db> {
    edges: FxHashSet<FlowEdge<'db>>,
    /// Definition references used to decide whether an unresolved flow can return to the root.
    definition_edges: Vec<(Definition<'db>, Definition<'db>)>,
    /// Definitions whose captured outer parameters cannot be mapped to their parent specialization.
    inconclusive_definitions: FxHashSet<Definition<'db>>,
    /// Whether a definition body or its formal parameters could not be inspected.
    inconclusive: bool,
}

/// Walks one identity-specialized definition body and records references as graph edges.
///
/// Referenced definitions are queued for a separate walk instead of being expanded here.
struct SpecializationFlowVisitor<'db> {
    source_parameters: FxHashSet<BoundTypeVarIdentity<'db>>,
    env: ProgramEnvironment<'db>,
    visited_types: TypeCollector<'db>,
    edges: RefCell<Vec<FlowEdge<'db>>>,
    referenced_definitions: RefCell<Vec<RecursiveDefinition<'db>>>,
    inconclusive: Cell<bool>,
}

/// Finds which parameters of the current source definition occur in one actual argument.
struct SourceParameterCollector<'a, 'db> {
    source_parameters: &'a FxHashSet<BoundTypeVarIdentity<'db>>,
    env: &'a ProgramEnvironment<'db>,
    found: RefCell<FxHashMap<BoundTypeVarIdentity<'db>, bool>>,
    visited_types: TypeCollector<'db>,
    in_nested_type: Cell<bool>,
}

impl<'db> RecursiveDefinition<'db> {
    fn from_type(db: &'db dyn Db, ty: Type<'db>) -> Option<DefinitionUse<'db>> {
        let (target, specialization) = match ty {
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
            _ => return None,
        };

        let specialization = match target.generic_context(db) {
            Some(generic_context) => Some(
                specialization
                    .unwrap_or_else(|| target.default_specialization(db, generic_context)),
            ),
            None => specialization,
        };
        Some(DefinitionUse {
            target,
            specialization,
        })
    }

    fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        match self {
            Self::TypeAlias(alias) => alias.definition(db),
            Self::Structural(recursive) => recursive.definition(db),
            Self::Protocol(origin) | Self::TypedDict(origin) => origin.definition(db),
            Self::Callable(callable, _) => callable.origin().definition(db),
        }
    }

    fn generic_context(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        match self {
            Self::TypeAlias(alias) => alias.generic_context(db),
            Self::Structural(recursive) => recursive.parameters(db),
            Self::Protocol(origin) | Self::TypedDict(origin) => origin.generic_context(db),
            Self::Callable(callable, _) => callable.origin().generic_context(db),
        }
    }

    fn default_specialization(
        self,
        db: &'db dyn Db,
        generic_context: GenericContext<'db>,
    ) -> Specialization<'db> {
        let known_class = match self {
            Self::TypeAlias(_) | Self::Structural(_) => None,
            Self::Protocol(origin) | Self::TypedDict(origin) => origin.known(db),
            Self::Callable(callable, _) => callable.origin().known(db),
        };
        generic_context.default_specialization(db, known_class)
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
    fn parameters(self, db: &'db dyn Db) -> impl Iterator<Item = BoundTypeVarIdentity<'db>> {
        self.generic_context(db)
            .into_iter()
            .flat_map(|context| context.variables(db))
            .map(move |parameter| Self::parameter_identity(db, parameter))
    }

    /// Returns `None` if two formal parameters share an identity, since their flows could not be
    /// distinguished.
    fn source_parameters(self, db: &'db dyn Db) -> Option<FxHashSet<BoundTypeVarIdentity<'db>>> {
        let mut parameters = FxHashSet::default();
        for identity in self.parameters(db) {
            if !parameters.insert(identity) {
                return None;
            }
        }
        Some(parameters)
    }

    /// A false result proves that reachable specializations are bounded. A true result also
    /// includes incomplete dependency discovery, and does not establish growth on any given path.
    fn may_have_unbounded_specialization(self, db: &'db dyn Db) -> bool {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, _, _, ()| true,
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn specialization_flow_inner<'db>(
            db: &'db dyn Db,
            root: RecursiveDefinition<'db>,
            _: (),
        ) -> bool {
            let graph = SpecializationFlowGraph::build(db, root);
            graph.root_may_have_unbounded_specialization(db, root)
        }

        specialization_flow_inner(db, self, ())
    }
}

impl<'db> DefinitionUse<'db> {
    /// Compare corresponding arguments, retaining their structure and positions. Every argument
    /// must embed its previous value, and at least one must have acquired additional structure.
    fn structurally_expands(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        embedding: &TypeEmbedding<'db>,
    ) -> bool {
        let (Some(current), Some(previous)) = (self.specialization, previous.specialization) else {
            return false;
        };
        let current = current.types(db);
        let previous = previous.types(db);
        current != previous
            && current.len() == previous.len()
            && previous
                .iter()
                .zip(current)
                .all(|(&previous, &current)| embedding.embeds(db, env, previous, current))
    }

    fn has_shrinking_argument(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: &(impl Iterator<Item = Self> + Clone),
    ) -> bool {
        let Some(current) = self.specialization else {
            return false;
        };
        let current = current.types(db);
        current.iter().enumerate().any(|(index, &argument)| {
            let current_size = TypeStructureSize::of(db, env, argument);
            previous.clone().all(|previous| {
                let Some(previous) = previous
                    .specialization
                    .and_then(|specialization| specialization.types(db).get(index).copied())
                else {
                    return false;
                };
                current_size < TypeStructureSize::of(db, env, previous)
            })
        })
    }

    fn walk_arguments(self, db: &'db dyn Db, visitor: &impl TypeVisitor<'db>) {
        if let Some(specialization) = self.specialization {
            for argument in specialization.types(db) {
                visitor.visit_type(db, *argument);
            }
        }
    }
}

/// Measures the finite stored structure of an argument, including each element of flattened
/// tuples and parameter lists. Following declaration bodies here would expand the very recursion
/// whose progress we are measuring.
pub(super) struct TypeStructureSize<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    size: Cell<usize>,
    visited: TypeCollector<'db>,
}

impl<'db> TypeStructureSize<'_, 'db> {
    pub(super) fn of(db: &'db dyn Db, env: &ProgramEnvironment<'db>, ty: Type<'db>) -> usize {
        let visitor = TypeStructureSize {
            env,
            size: Cell::new(0),
            visited: TypeCollector::default(),
        };
        visitor.visit_type(db, ty);
        visitor.size.get()
    }
}

impl<'db> TypeVisitor<'db> for TypeStructureSize<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        self.size.set(self.size.get().saturating_add(1));
        walk_type_with_recursion_guard(db, ty, self, &self.visited);
    }
    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        if let Some(specialization) = alias.specialization(db) {
            for &argument in specialization.types(db) {
                self.visit_type(db, argument);
            }
        }
    }
    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if let Some(specialization) = recursive.arguments(db) {
            for &argument in specialization.types(db) {
                self.visit_type(db, argument);
            }
        }
    }
    fn visit_bound_type_var_type(&self, _db: &'db dyn Db, _typevar: BoundTypeVarInstance<'db>) {}
}

/// Homeomorphic embedding of stored type structure. A type embeds an earlier type if the earlier
/// type can be obtained by removing wrappers or children, without changing the remaining leaves
/// or their order. For example, `list[int]` embeds `int`, and `tuple[int, str, bytes]` embeds
/// `tuple[int, bytes]`, but `tuple[str, int]` does not embed `tuple[int, str]`.
///
/// This is a structural growth test, not assignability or a proof of nontermination. In particular,
/// it never identifies a permutation of equally sized arguments as growth. Exact repetitions are
/// handled by the caller's cycle detector. Declaration bodies are not expanded: the finite stored
/// arguments are the state being compared, even when they refer to recursive declarations.
///
/// With a finite set of declarations and leaves, an infinite sequence of trees eventually embeds
/// an earlier tree. Comparing ordered children as subsequences also covers variadic arguments,
/// whose growth may increase arity instead of nesting depth.
#[derive(Debug, Default)]
struct TypeEmbedding<'db> {
    comparisons: RefCell<FxHashMap<(Type<'db>, Type<'db>), bool>>,
}

impl<'db> TypeEmbedding<'db> {
    fn embeds(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        current: Type<'db>,
    ) -> bool {
        if previous == current {
            return true;
        }
        if let Some(&result) = self.comparisons.borrow().get(&(previous, current)) {
            return result;
        }

        let current_children = StoredTypeChildren::of(db, env, current);
        let result = current_children
            .iter()
            .any(|&child| self.embeds(db, env, previous, child))
            || (Self::same_constructor(db, env, previous, current) && {
                let previous_children = StoredTypeChildren::of(db, env, previous);
                // Distinct types with identical children may differ in non-type metadata. They
                // do not establish growth: at least one wrapper or child must have been added.
                let mut remaining = current_children.iter();
                previous_children != current_children
                    && previous_children.iter().all(|&previous| {
                        remaining.any(|&current| self.embeds(db, env, previous, current))
                    })
            });
        self.comparisons
            .borrow_mut()
            .insert((previous, current), result);
        result
    }

    fn same_constructor(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        current: Type<'db>,
    ) -> bool {
        match (previous, current) {
            (Type::GenericAlias(a), Type::GenericAlias(b)) => {
                a.origin(db) == b.origin(db)
                    && a.specialization(db).materialization_kind(db)
                        == b.specialization(db).materialization_kind(db)
            }
            (Type::NominalInstance(a), Type::NominalInstance(b)) => {
                a.class_literal(db, env) == b.class_literal(db, env)
            }
            (Type::FunctionLiteral(a), Type::FunctionLiteral(b)) => a.literal(db) == b.literal(db),
            (Type::TypeAlias(a), Type::TypeAlias(b)) => a.definition(db) == b.definition(db),
            (Type::Recursive(a), Type::Recursive(b)) => a.definition(db) == b.definition(db),
            (Type::ProtocolInstance(a), Type::ProtocolInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            (Type::TypedDict(a), Type::TypedDict(b)) => a.definition(db) == b.definition(db),
            (Type::NewTypeInstance(a), Type::NewTypeInstance(b)) => {
                a.definition(db) == b.definition(db)
            }
            _ => mem::discriminant(&previous) == mem::discriminant(&current),
        }
    }
}

/// Collect immediate stored children in order, retaining duplicates. Type variables and nominal
/// declarations are leaves; visiting their bounds or bodies would introduce semantic expansion
/// into the structural comparison itself.
struct StoredTypeChildren<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    children: RefCell<SmallVec<[Type<'db>; 4]>>,
}

impl<'db> StoredTypeChildren<'_, 'db> {
    fn of(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> SmallVec<[Type<'db>; 4]> {
        let visitor = StoredTypeChildren {
            env,
            children: RefCell::default(),
        };
        if let TypeKind::NonAtomic(ty) = TypeKind::from(ty) {
            walk_non_atomic_type(db, ty, &visitor);
        }
        visitor.children.into_inner()
    }
}

impl<'db> TypeVisitor<'db> for StoredTypeChildren<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }
    fn visit_type(&self, _db: &'db dyn Db, ty: Type<'db>) {
        self.children.borrow_mut().push(ty);
    }
    fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
        walk_specialization_types(db, alias.specialization(db), self);
    }
    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        if let Some(specialization) = alias.specialization(db) {
            walk_specialization_types(db, specialization, self);
        }
    }
    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if let Some(specialization) = recursive.arguments(db) {
            walk_specialization_types(db, specialization, self);
        }
    }
    fn visit_bound_type_var_type(&self, _db: &'db dyn Db, _typevar: BoundTypeVarInstance<'db>) {}
    fn visit_type_var_type(&self, _db: &'db dyn Db, _typevar: TypeVarInstance<'db>) {}
}

impl<'db> SpecializationFlowGraph<'db> {
    fn build(db: &'db dyn Db, root: RecursiveDefinition<'db>) -> Self {
        let mut graph = Self::default();
        let mut pending = vec![root];
        let mut visited = FxHashSet::default();

        while let Some(source) = pending.pop() {
            let source_definition = source.definition(db);
            if !visited.insert(source) {
                continue;
            }
            let Some(visitor) = SpecializationFlowVisitor::new(db, source) else {
                graph.inconclusive = true;
                continue;
            };
            if !visitor.visit_definition_body(db, source) {
                graph.inconclusive = true;
            }
            graph.edges.extend(visitor.edges.into_inner());
            if visitor.inconclusive.get() {
                graph.inconclusive_definitions.insert(source_definition);
            }
            let referenced_definitions = visitor.referenced_definitions.into_inner();
            graph.definition_edges.extend(
                referenced_definitions
                    .iter()
                    .map(|target| (source_definition, target.definition(db))),
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

        let root_definition = root.definition(db);
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
        additional_parameters: impl IntoIterator<Item = BoundTypeVarIdentity<'db>>,
    ) -> FxHashMap<BoundTypeVarIdentity<'db>, usize> {
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

    fn inconclusive_definition_reaches(&self, target: Definition<'db>) -> bool {
        if self.inconclusive_definitions.is_empty() {
            return false;
        }

        let definitions_reaching_target = self.definitions_reaching(target);
        !self
            .inconclusive_definitions
            .is_disjoint(&definitions_reaching_target)
    }

    fn definition_reaches(&self, from: Definition<'db>, to: Definition<'db>) -> bool {
        self.definitions_reaching(to).contains(&from)
    }

    fn definitions_reaching(&self, target: Definition<'db>) -> FxHashSet<Definition<'db>> {
        let mut incoming = FxHashMap::<Definition, SmallVec<[Definition; 2]>>::default();
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
    fn new(db: &'db dyn Db, source: RecursiveDefinition<'db>) -> Option<Self> {
        Some(Self {
            source_parameters: source.source_parameters(db)?,
            env: ProgramEnvironment::from_definition(source.definition(db)),
            visited_types: TypeCollector::default(),
            edges: RefCell::default(),
            referenced_definitions: RefCell::default(),
            inconclusive: Cell::default(),
        })
    }

    /// Visits the definition with each formal parameter mapped to itself.
    fn visit_definition_body(&self, db: &'db dyn Db, source: RecursiveDefinition<'db>) -> bool {
        match source {
            RecursiveDefinition::Structural(recursive) => {
                self.visit_type(db, recursive.unfold(db, &self.env).into_type());
            }
            RecursiveDefinition::TypeAlias(alias) => {
                self.visit_type(db, alias.raw_value_type(db));
            }
            RecursiveDefinition::Protocol(origin) => {
                let Some(protocol) = origin.identity_specialization(db).into_protocol_class(db)
                else {
                    return false;
                };
                protocol.walk_recursive_member_types(db, self);
            }
            RecursiveDefinition::TypedDict(origin) => {
                let typed_dict = TypedDictType::new(origin.identity_specialization(db));
                for field in typed_dict.items(db).values() {
                    self.visit_type(db, field.declared_ty);
                }
                if let Some(extra_items) = typed_dict.explicit_extra_items(db) {
                    self.visit_type(db, extra_items.declared_ty);
                }
            }
            RecursiveDefinition::Callable(callable, mode) => {
                let dependencies = CallableDependencies::default();
                dependencies.visit(db, &self.env, callable.identity_type(db, &self.env), mode);
                let mut complete = !dependencies.inconclusive.get();
                for reference in dependencies.references.into_inner() {
                    // A helper may dispatch through its remaining arguments. Analyzing that
                    // helper independently loses those arguments, so it cannot establish that
                    // expansion will not return to the root.
                    complete &= reference.target == source;
                    self.record_reference(db, reference);
                }
                return complete;
            }
        }
        true
    }

    fn record_reference(&self, db: &'db dyn Db, reference: DefinitionUse<'db>) {
        self.referenced_definitions
            .borrow_mut()
            .push(reference.target);

        let Some(target_context) = reference.target.generic_context(db) else {
            if reference.specialization.is_some() {
                self.inconclusive.set(true);
            }
            return;
        };
        let Some(specialization) = reference.specialization else {
            self.inconclusive.set(true);
            return;
        };
        if specialization.generic_context(db) != target_context {
            self.inconclusive.set(true);
            return;
        }

        let target_parameters = reference.target.parameters(db).collect::<Vec<_>>();
        let arguments = specialization.types(db);
        if target_parameters.len() != arguments.len() {
            self.inconclusive.set(true);
            return;
        }

        for (target, argument) in target_parameters.into_iter().zip(arguments.iter().copied()) {
            for (from, kind) in
                SourceParameterCollector::classify(db, &self.env, &self.source_parameters, argument)
            {
                self.edges.borrow_mut().push(FlowEdge {
                    from,
                    to: target,
                    kind,
                });
            }
        }
    }
}

impl<'db> TypeVisitor<'db> for SpecializationFlowVisitor<'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        &self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if let Type::TypeVar(typevar) = ty {
            let identity = RecursiveDefinition::parameter_identity(db, typevar);
            if !self.source_parameters.contains(&identity) {
                // Nested definitions can capture a type variable from an outer generic scope.
                // Specialization does not yet retain the parent mapping needed to model it.
                self.inconclusive.set(true);
            }
            return;
        }

        if let Some(reference) = RecursiveDefinition::from_type(db, ty) {
            self.record_reference(db, reference);
            reference.walk_arguments(db, self);
            return;
        }

        walk_type_with_recursion_guard(db, ty, self, &self.visited_types);
    }

    fn visit_bound_type_var_type(
        &self,
        _db: &'db dyn Db,
        _bound_typevar: BoundTypeVarInstance<'db>,
    ) {
    }
}

impl<'a, 'db> SourceParameterCollector<'a, 'db> {
    fn classify(
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
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
        collector.visit_type(db, argument);
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

impl<'db> TypeVisitor<'db> for SourceParameterCollector<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
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
            Type::Union(union) => {
                self.visit_union_type(db, union);
                return;
            }
            Type::Intersection(intersection) => {
                self.visit_intersection_type(db, intersection);
                return;
            }
            Type::SubclassOf(subclass_of) if let Some(typevar) = subclass_of.into_type_var() => {
                // Repeated metaclass projection reaches `type`; it does not
                // accumulate layers like `list[T]` or `tuple[T]`.
                self.visit_type(db, Type::TypeVar(typevar));
                return;
            }
            _ => {}
        }

        let was_in_nested_type = self.in_nested_type.replace(true);
        if let Some(reference) = RecursiveDefinition::from_type(db, ty) {
            reference.walk_arguments(db, self);
        } else {
            walk_type_with_recursion_guard(db, ty, self, &self.visited_types);
        }
        self.in_nested_type.set(was_in_nested_type);
    }

    fn visit_bound_type_var_type(
        &self,
        _db: &'db dyn Db,
        _bound_typevar: BoundTypeVarInstance<'db>,
    ) {
    }
}

impl<'db> TypeAliasType<'db> {
    /// Returns whether this alias can refer back to its own definition.
    pub(crate) fn is_recursive(self, db: &'db dyn Db) -> bool {
        let root = RecursiveDefinition::TypeAlias(self.unspecialized(db));
        let root_definition = root.definition(db);
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

    /// Returns an identity that remains stable while this item is active in a [`CycleDetector`].
    fn to_identity(&self, db: &'db dyn Db) -> Self::Id;
}

impl<'db> HasIdentity<'db> for Type<'db> {
    type Id = TypeIdentity<'db>;

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.may_share_type_identity(db, *other)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        Type::to_type_identity(*self, db)
    }
}

pub(crate) type PairVisitor<'db, Tag, C> = CycleDetector<'db, Tag, (Type<'db>, Type<'db>), C, 1>;

impl<'db> HasIdentity<'db> for (Type<'db>, Type<'db>) {
    type Id = (TypeIdentity<'db>, TypeIdentity<'db>);

    fn may_share_identity(&self, db: &'db dyn Db, other: &Self) -> bool {
        self.0.may_share_type_identity(db, other.0) && self.1.may_share_type_identity(db, other.1)
    }

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (self.0.to_type_identity(db), self.1.to_type_identity(db))
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

    fn to_identity(&self, db: &'db dyn Db) -> Self::Id {
        (
            self.0.to_type_identity(db),
            self.1,
            self.2.to_type_identity(db),
        )
    }
}

/// `CycleDetector` is temporary, so callers should choose the capacity that keeps observed cycle
/// paths inline even when that makes `seen` slightly larger than an `FxIndexSet<T>`.
#[derive(Debug)]
pub struct CycleDetector<'db, Tag, T: HasIdentity<'db>, R, const INLINE_CAPACITY: usize> {
    /// The active recursion stack and the lazily-computed identity of each item.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[ActiveCycleDetectorVisit<'db, T>; INLINE_CAPACITY]>>,

    /// Memoized results from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<T, R>>,

    fallback: R,

    _tag: PhantomData<fn() -> &'db Tag>,
}

impl<'db, Tag, T, R, const INLINE_CAPACITY: usize> CycleDetector<'db, Tag, T, R, INLINE_CAPACITY>
where
    T: HasIdentity<'db>,
{
    pub(crate) fn new(fallback: R) -> Self {
        CycleDetector {
            seen: RefCell::new(SmallVec::new()),
            cache: RefCell::new(CycleDetectorCache::new()),
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
    pub fn visit(&self, db: &'db dyn Db, item: T, compute: impl FnOnce() -> R) -> R {
        match self.begin_visit(db, item) {
            CycleDetectorVisit::Ready(result) => result,
            CycleDetectorVisit::Cycle(_) => self.fallback.clone(),
            CycleDetectorVisit::Pending(item) => {
                let result = compute();
                self.finish_visit(item, result)
            }
        }
    }

    /// Visits `item`, returning it in `Err` if another active item has the same identity.
    ///
    /// The caller must convert `Err(item)` into an operation-specific conservative result. An
    /// exact recursive reentry uses the detector's configured fallback and is returned as `Ok`.
    ///
    /// Completed results are reused only when `reuse_cached` accepts them. Otherwise, the visit
    /// recomputes the result using the same active recursion guards, without replacing the cached
    /// value. Results for previously uncached items are memoized as usual.
    #[inline]
    pub(super) fn try_visit(
        &self,
        db: &'db dyn Db,
        item: T,
        reuse_cached: impl FnOnce(&R) -> bool,
        compute: impl FnOnce() -> R,
    ) -> Result<R, T> {
        let cached_result = self.cache.borrow().get(&item).cloned();
        let was_cached = cached_result.is_some();
        if let Some(result) = cached_result
            && reuse_cached(&result)
        {
            return Ok(result);
        }

        match self.begin_active_visit(db, item) {
            CycleDetectorVisit::Ready(result) => Ok(result),
            CycleDetectorVisit::Cycle(item) => Err(item),
            CycleDetectorVisit::Pending(item) => {
                let result = compute();
                if was_cached {
                    self.finish_active_visit(&item);
                    Ok(result)
                } else {
                    Ok(self.finish_visit(item, result))
                }
            }
        }
    }

    fn begin_visit(&self, db: &'db dyn Db, item: T) -> CycleDetectorVisit<T, R> {
        if let Some(result) = self.cache.borrow().get(&item) {
            return CycleDetectorVisit::Ready(result.clone());
        }

        self.begin_active_visit(db, item)
    }

    fn begin_active_visit(&self, db: &'db dyn Db, item: T) -> CycleDetectorVisit<T, R> {
        let seen = self.seen.borrow();
        if seen.iter().any(|active| active.item == item) {
            return CycleDetectorVisit::Ready(self.fallback.clone());
        }

        let mut candidates = seen
            .iter()
            .filter(|active| item.may_share_identity(db, &active.item))
            .peekable();
        let identity = if candidates.peek().is_none() {
            OnceCell::new()
        } else {
            // Deriving an identity can require a structural definition walk. Defer it until a
            // cheap candidate match shows that another active item could form a cycle.
            let identity = item.to_identity(db);
            if candidates.any(|active| {
                active.identity.get_or_init(|| active.item.to_identity(db)) == &identity
            }) {
                return CycleDetectorVisit::Cycle(item);
            }
            OnceCell::from(identity)
        };
        drop(seen);

        self.seen.borrow_mut().push(ActiveCycleDetectorVisit {
            item: item.clone(),
            identity,
        });
        CycleDetectorVisit::Pending(item)
    }

    /// Finish a [`CycleDetectorVisit::Pending`] visit and cache its result.
    fn finish_visit(&self, item: T, result: R) -> R {
        self.finish_active_visit(&item);
        self.cache
            .borrow_mut()
            .insert_completed(item, result.clone());
        result
    }

    fn finish_active_visit(&self, item: &T) {
        let active = self.seen.borrow_mut().pop();
        debug_assert!(active.as_ref().is_some_and(|active| active.item == *item));
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
pub(super) enum CycleDetectorVisit<T, R> {
    /// The item already has a completed result or hit an exact recursive edge.
    Ready(R),
    /// A different item with the same abstract identity is already pending.
    Cycle(T),
    /// The caller should compute the result and finish the pending visit.
    Pending(T),
}

/// Guards recursive type transformations.
pub(crate) struct TypeTransformer<'db, Tag> {
    /// The active transformation stack and its recursive identities.
    /// Completed visits are removed from the end of the stack.
    seen: RefCell<SmallVec<[ActiveTypeTransformation<'db>; 3]>>,

    /// Memoized transformations from earlier visits in the current recursive operation.
    cache: RefCell<CycleDetectorCache<Type<'db>, Type<'db>>>,

    _tag: PhantomData<fn() -> Tag>,
}

impl<Tag> Default for TypeTransformer<'_, Tag> {
    fn default() -> Self {
        Self {
            seen: RefCell::default(),
            cache: RefCell::default(),
            _tag: PhantomData,
        }
    }
}

impl<'db, Tag> TypeTransformer<'db, Tag> {
    #[inline]
    pub(crate) fn visit_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        compute: impl FnOnce() -> Type<'db>,
    ) -> Type<'db> {
        match self.begin_visit(db, ty) {
            TypeTransformerVisit::Ready(result) => result,
            TypeTransformerVisit::Pending(ty) => {
                let result = compute();
                self.finish_visit(ty, result)
            }
        }
    }

    fn begin_visit(&self, db: &'db dyn Db, ty: Type<'db>) -> TypeTransformerVisit<'db> {
        if let Some(result) = self.cache.borrow().get(&ty) {
            return TypeTransformerVisit::Ready(*result);
        }

        let identity = ty.to_type_identity(db);
        let seen = self.seen.borrow();
        if seen
            .iter()
            .any(|active| active.ty == ty || active.identity == identity)
        {
            return TypeTransformerVisit::Ready(ty);
        }
        drop(seen);

        self.seen
            .borrow_mut()
            .push(ActiveTypeTransformation { ty, identity });
        TypeTransformerVisit::Pending(ty)
    }

    fn finish_visit(&self, ty: Type<'db>, result: Type<'db>) -> Type<'db> {
        let active = self.seen.borrow_mut().pop();
        debug_assert_eq!(active.map(|active| active.ty), Some(ty));
        self.cache.borrow_mut().insert_completed(ty, result);
        result
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveTypeTransformation<'db> {
    ty: Type<'db>,
    identity: TypeIdentity<'db>,
}

enum TypeTransformerVisit<'db> {
    Ready(Type<'db>),
    Pending(Type<'db>),
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
enum CycleDetectorCache<T, R> {
    #[default]
    Empty,
    One((T, R)),
    Two([(T, R); 2]),
    Spilled(FxHashMap<T, R>),
}

impl<T, R> CycleDetectorCache<T, R> {
    const fn new() -> Self {
        Self::Empty
    }

    fn get(&self, item: &T) -> Option<&R>
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
    seen: RefCell<FxHashSet<T>>,
}

impl<T> Default for ActiveRecursionDetector<T> {
    fn default() -> Self {
        Self {
            seen: RefCell::new(FxHashSet::default()),
        }
    }
}

impl<T: Hash + Eq + Clone> ActiveRecursionDetector<T> {
    pub(crate) fn is_empty(&self) -> bool {
        self.seen.borrow().is_empty()
    }

    pub(crate) fn visit<R>(
        &self,
        item: &T,
        on_cycle: impl FnOnce() -> R,
        func: impl FnOnce() -> R,
    ) -> R {
        if !self.seen.borrow_mut().insert(item.clone()) {
            return on_cycle();
        }

        // Keep the active-recursion state scoped even if `func` unwinds. In some cases, we catch
        // panics and continue handling later work on the same thread.
        let _guard = ActiveRecursionGuard {
            seen: &self.seen,
            item,
        };

        func()
    }
}

struct ActiveRecursionGuard<'a, T: Hash + Eq> {
    seen: &'a RefCell<FxHashSet<T>>,
    item: &'a T,
}

impl<T: Hash + Eq> Drop for ActiveRecursionGuard<'_, T> {
    fn drop(&mut self) {
        self.seen.borrow_mut().remove(self.item);
    }
}

/// Discovers callable dependencies without constructing signatures or call bindings.
///
/// The walk follows constructor members and `__call__`, but not the argument or return types of
/// ordinary functions: those do not cause further callable expansion. Helpers retain their actual
/// arguments, so `Forward[T].__new__: type[T]` exposes `T`, while a class that ignores `T` does not.
#[derive(Default)]
struct CallableDependencies<'db> {
    inconclusive: Cell<bool>,
    visited: RefCell<FxHashSet<Type<'db>>>,
    active: ActiveRecursionDetector<DefinitionUse<'db>>,
    identities: ActiveRecursionDetector<TypeIdentity<'db>>,
    references: RefCell<Vec<DefinitionUse<'db>>>,
}

impl<'db> CallableDependencies<'db> {
    fn visit(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        mode: CallableExpansion,
    ) {
        if !self.visited.borrow_mut().insert(ty) {
            return;
        }
        let Some(reference) = CallableDefinition::from_type(db, env, ty, mode) else {
            self.identities.visit(
                &ty.to_type_identity(db),
                || (),
                || self.visit_body(db, env, ty, mode),
            );
            return;
        };
        let stop = {
            // Repeated forwarding helpers may unwrap an argument while changing the others.
            // Continue only if one argument is below every active value for that parameter.
            // Establishing a new minimum keeps symbolic exploration finite without a depth limit.
            let active = self.active.seen.borrow();
            let previous = active
                .iter()
                .copied()
                .filter(|active| active.target == reference.target);
            previous.clone().next().is_some()
                && !reference.has_shrinking_argument(db, env, &previous)
        };
        if stop {
            self.references.borrow_mut().push(reference);
            return;
        }
        self.active
            .visit(&reference, || (), || self.visit_body(db, env, ty, mode));
    }

    fn visit_body(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        mode: CallableExpansion,
    ) {
        let visit = |ty| self.visit(db, env, ty, mode);
        if let Some(fallback) = ty.materialized_divergent_fallback() {
            visit(fallback);
            return;
        }
        let constructor = match ty {
            Type::ClassLiteral(class) => {
                let class = class.identity_specialization(db);
                Some((class, Type::from(class)))
            }
            Type::GenericAlias(alias) => Some((ClassType::Generic(alias), ty)),
            Type::SubclassOf(subclass) => match subclass.subclass_of() {
                SubclassOfInner::Class(class) => Some((
                    class,
                    match mode {
                        CallableExpansion::Bindings => ty,
                        CallableExpansion::Upcast => Type::from(class),
                    },
                )),
                SubclassOfInner::Protocol(protocol) => protocol.class_origin(db).map(|origin| {
                    let class = *origin;
                    let receiver = if mode == CallableExpansion::Bindings
                        || protocol.materialization_kind(db).is_some()
                    {
                        ty
                    } else {
                        Type::from(class)
                    };
                    (class, receiver)
                }),
                SubclassOfInner::TypeVar(typevar) => {
                    match typevar.require_bound_or_constraints(db, env) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => {
                            visit(bound.constructor_for_typevar_bound(db, env));
                        }
                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            for constraint in constraints.elements(db) {
                                visit(constraint.to_meta_type(db, env));
                            }
                        }
                    }
                    None
                }
                SubclassOfInner::Dynamic(_) => None,
            },
            Type::BoundMethod(method) => {
                visit(method.func(db));
                None
            }
            Type::KnownBoundMethod(KnownBoundMethodType::DunderCall(callable)) => {
                visit(callable.inner(db));
                None
            }
            Type::KnownInstance(KnownInstanceType::MethodWrapper(wrapper)) => {
                if wrapper.kind(db) == MethodWrapperKind::Staticmethod {
                    visit(wrapper.wrapped(db));
                }
                None
            }
            Type::NewTypeInstance(newtype) if mode == CallableExpansion::Upcast => {
                visit(newtype.concrete_base_type(db));
                None
            }
            Type::NominalInstance(_) | Type::ProtocolInstance(_) | Type::NewTypeInstance(_) => {
                let member = ty
                    .member_lookup_with_policy_and_receiver(
                        db,
                        env,
                        "__call__",
                        MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                        None,
                    )
                    .unwrap_or_else(|error| error.fallback_member(db));
                self.visit_member(
                    db,
                    env,
                    member.member(db).place,
                    member.descriptor_origin(db),
                    mode,
                );
                None
            }
            Type::TypeAlias(alias) => {
                visit(alias.value_type(db));
                None
            }
            Type::Recursive(recursive) => {
                if let Some(unfolded) = recursive.unfold(db, env).into_unfolded() {
                    visit(unfolded);
                }
                None
            }
            Type::Union(union) => {
                for &element in union.elements(db) {
                    visit(element);
                }
                None
            }
            Type::Intersection(intersection) => {
                for element in intersection.positive_elements_or_object(db) {
                    visit(element);
                }
                None
            }
            Type::TypeVar(typevar) => {
                if mode == CallableExpansion::Bindings {
                    match typevar.require_bound_or_constraints(db, env) {
                        TypeVarBoundOrConstraints::UpperBound(bound) => visit(bound),
                        TypeVarBoundOrConstraints::Constraints(constraints) => {
                            for &constraint in constraints.elements(db) {
                                visit(constraint);
                            }
                        }
                    }
                }
                None
            }
            Type::LiteralValue(literal) => {
                if let LiteralValueTypeKind::Enum(literal) = literal.kind() {
                    visit(literal.enum_class_instance(db, env));
                }
                None
            }
            Type::EnumComplement(complement) => {
                visit(complement.remaining_literal_union(db, env));
                None
            }
            Type::KnownInstance(
                KnownInstanceType::NewType(_)
                | KnownInstanceType::FunctoolsPartial(_)
                | KnownInstanceType::FunctoolsPartialCall(_),
            ) => None,
            Type::KnownInstance(instance) => {
                if mode == CallableExpansion::Bindings {
                    visit(instance.instance_fallback(db, env));
                }
                None
            }
            // These types have no constructor dependencies. Function signatures are leaves even
            // when their annotations refer back to the class being constructed.
            Type::FunctionLiteral(_)
            | Type::Callable(_)
            | Type::KnownBoundMethod(_)
            | Type::WrapperDescriptor(_)
            | Type::Dynamic(_)
            | Type::Divergent(_)
            | Type::Never
            | Type::AlwaysTruthy
            | Type::AlwaysFalsy
            | Type::RecursiveVar(_)
            | Type::TypeIs(_)
            | Type::TypeGuard(_)
            | Type::TypeForm(_)
            | Type::TypedDict(_)
            | Type::DataclassTransformer(_)
            | Type::DataclassDecorator(_)
            | Type::ModuleLiteral(_)
            | Type::SpecialForm(_)
            | Type::PropertyInstance(_)
            | Type::SlotDescriptor(_)
            | Type::BoundSuper(_) => None,
        };
        if let Some((class, receiver)) = constructor {
            let members = ConstructorMembers::new(db, env, class, receiver);
            // Include every possible constructor stage. Choosing a stage can depend on call-time
            // overload resolution, so dependency discovery must not discard downstream stages.
            for member in [
                members.metaclass_call(db, env),
                members.new_method(db, env),
                members.initializer(db, env, false),
            ] {
                self.visit_member(db, env, member.place, member.origin, mode);
            }
        }
    }

    fn visit_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        place: Place<'db>,
        origin: DescriptorOrigin<'db>,
        mode: CallableExpansion,
    ) {
        // Resolving a symbolic receiver can select a different descriptor overload from a
        // concrete specialization. Such a call cannot prove that specialization is bounded.
        if origin.dispatches.is_some() {
            self.inconclusive.set(true);
        } else if let Some(ty) = place.ignore_possibly_undefined() {
            self.visit(db, env, ty, mode);
        }
    }
}

/// Tracks callable expansion along one path. Exact cycles and growing specializations have
/// separate recovery values: an exact cycle can use the operation's fixed-point seed, whereas
/// a changing specialization needs a gradual approximation.
#[derive(Debug, Default)]
pub(super) struct CallableRecursionGuard<'db> {
    mode: CallableExpansion,
    active: ActiveRecursionDetector<Type<'db>>,
    identities: ActiveRecursionDetector<TypeIdentity<'db>>,
    growth: CallableGrowthDetector<'db>,
    cache: ConstructorCallableCache<'db>,
}

/// Constructor results that used an ancestor-dependent fallback cannot be reused on another
/// path. The recovery counter invalidates only those results, preserving memoization of siblings.
#[derive(Debug, Default)]
struct ConstructorCallableCache<'db> {
    use_shared_cache: bool,
    recoveries: Cell<usize>,
    constructors: RefCell<CycleDetectorCache<(ClassType<'db>, Type<'db>), CallableTypes<'db>>>,
}

impl<'db> ConstructorCallableCache<'db> {
    fn get_or_insert(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
        receiver: Type<'db>,
        expand: impl FnOnce() -> CallableTypes<'db>,
    ) -> CallableTypes<'db> {
        if self.use_shared_cache {
            return class.into_callable_with_receiver(db, receiver);
        }
        let key = (class, receiver);
        if let Some(callables) = self.constructors.borrow().get(&key) {
            return callables.clone();
        }

        let recoveries = self.recoveries.get();
        let callables = expand();
        if self.recoveries.get() == recoveries {
            self.constructors
                .borrow_mut()
                .insert_completed(key, callables.clone());
        }
        callables
    }
}

/// Approximates expansion only after finding structural growth relative to an active invocation.
/// Bounded specialization proven by the dependency graph needs only exact cycle detection.
/// Otherwise, every argument must embed its earlier value, with at least one strict expansion.
/// Shrinking and incomparable states, including argument permutations, continue to be explored.
///
/// Structural growth is not a proof of an infinite computation: descriptor overloads can end a
/// growing chain. Relation observations postpone approximation while those overloads distinguish
/// successive states, including progress towards overloads that have not matched yet. Equal
/// observations alone never justify stopping. When both growth and repeated observations occur,
/// the gradual fallback can still lose diagnostics on finite chains outside this abstraction.
///
/// Each descriptor declaration gets a fixed observation vocabulary from its first invocation on
/// the active path. Later invocations use their actual specialized signatures, but record only
/// comparisons involving that vocabulary. This separates specialization needed for dispatch from
/// the finite state needed for termination: even the descriptor's own type arguments can grow.
#[derive(Debug, Default)]
struct CallableGrowthDetector<'db> {
    active: ActiveRecursionDetector<(DefinitionUse<'db>, DescriptorOrigin<'db>)>,
    embedding: TypeEmbedding<'db>,
    dispatch: Cell<DescriptorOrigin<'db>>,
    initial_dispatches: RefCell<FxHashMap<FunctionLiteral<'db>, DescriptorDispatches<'db>>>,
}

impl<'db> CallableGrowthDetector<'db> {
    fn should_approximate(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        reference: DefinitionUse<'db>,
    ) -> bool {
        let active = self.active.seen.borrow();
        let previous = active
            .iter()
            .filter(|(previous, _)| previous.target == reference.target);
        if previous.clone().next().is_none()
            || !reference.target.may_have_unbounded_specialization(db)
        {
            return false;
        }
        let current = self.dispatch.get();
        let initial_dispatches = self.initial_dispatches.borrow();
        previous.into_iter().any(|(previous, origin)| {
            reference.structurally_expands(db, env, *previous, &self.embedding)
                && same_descriptor_observations(db, env, &initial_dispatches, current, *origin)
        })
    }
}

/// Matching observations can permit approximation after structural growth has been established.
/// They do not imply equivalent future expansion behavior.
fn same_descriptor_observations<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    initial_dispatches: &FxHashMap<FunctionLiteral<'db>, DescriptorDispatches<'db>>,
    left: DescriptorOrigin<'db>,
    right: DescriptorOrigin<'db>,
) -> bool {
    if left == right {
        return true;
    }
    let (Some(left), Some(right)) = (left.dispatches, right.dispatches) else {
        return false;
    };
    let left = left.elements(db);
    let right = right.elements(db);
    left.len() == right.len()
        && left.iter().zip(right).all(|(&left, &right)| {
            if left.definitions(db) != right.definitions(db) {
                return false;
            }
            match (left.function(db), right.function(db)) {
                (Some(left_function), Some(right_function))
                    if left_function.literal(db) == right_function.literal(db) =>
                {
                    let Some(&initial) = initial_dispatches.get(&left_function.literal(db)) else {
                        return false;
                    };
                    left.observations(db, env.program(db), initial)
                        == right.observations(db, env.program(db), initial)
                }
                (None, None) => true,
                _ => false,
            }
        })
}

#[salsa::tracked]
impl<'db> DescriptorDispatch<'db> {
    fn arguments(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Iterator<Item = Type<'db>> {
        let call = self.call(db);
        self.bound_receiver(db).into_iter().chain([
            call.descriptor_type(db),
            call.instance(db).unwrap_or_else(|| Type::none(db, env)),
            call.owner(db),
        ])
    }

    /// Observe assignability against every current specialized overload, including rejected ones.
    /// A fixed vocabulary of types from the first invocation bounds the observations even when
    /// both the arguments and the signatures change. Nested obligations still distinguish progress
    /// towards a later match, such as list nesting that eventually satisfies a Sequence annotation.
    #[salsa::tracked(returns(ref), cycle_initial=|_, _, _, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
    fn observations(
        self,
        db: &'db dyn Db,
        program: Program<'db>,
        initial_dispatches: DescriptorDispatches<'db>,
    ) -> Box<[FxHashSet<RelationObservation<'db>>]> {
        let env = ProgramEnvironment::from_program(program);
        let Some(function) = self.function(db) else {
            return Box::default();
        };
        let patterns =
            descriptor_observation_patterns(db, program, initial_dispatches, function.literal(db));
        let mut observations = Vec::new();
        for signature in function.signature(db) {
            let inferable = signature
                .generic_context
                .map_or(TypeVarSet::None, |context| context.inferable_typevars(db));
            // A callable object used as `__get__` also binds its own `__call__` receiver.
            // Observe its specialized signature with that same complete argument list.
            let comparisons =
                self.arguments(db, &env)
                    .enumerate()
                    .filter_map(|(index, argument)| {
                        let parameter = signature.parameters().get_positional(index)?;
                        Some((index, argument, parameter.annotated_type()))
                    });
            let mut result =
                Type::assignability_observations(db, &env, comparisons, patterns, inferable);
            result.shrink_to_fit();
            observations.push(result);
        }
        observations.into_boxed_slice()
    }
}

/// The first invocation supplies concrete substitutions and input types that are absent from the
/// unspecialized declaration. Return annotations also matter: they can introduce a new descriptor
/// specialization farther down the chain. Include every alternative of this initial invocation so
/// union ordering cannot determine which specialization contributes the observation vocabulary.
#[salsa::tracked(returns(ref), cycle_initial=|_, _, _, _, _| FxHashSet::default(), heap_size=ruff_memory_usage::heap_size)]
fn descriptor_observation_patterns<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    initial_dispatches: DescriptorDispatches<'db>,
    literal: FunctionLiteral<'db>,
) -> FxHashSet<Type<'db>> {
    let env = ProgramEnvironment::from_program(program);
    let visitor = DescriptorPatternTypes {
        env: &env,
        types: RefCell::default(),
        active: ActiveRecursionDetector::default(),
    };
    for dispatch in initial_dispatches.elements(db) {
        if let Some(function) = dispatch.function(db)
            && function.literal(db) == literal
        {
            for argument in dispatch.arguments(db, &env) {
                visitor.visit_type(db, argument);
            }
            for signature in function.signature(db) {
                for parameter in signature.parameters() {
                    visitor.visit_type(db, parameter.annotated_type());
                }
                visitor.visit_type(db, signature.return_ty);
            }
        }
    }
    let mut types = visitor.types.into_inner();
    types.shrink_to_fit();
    types
}

/// Enumerate a fixed invocation's types without traversing nominal class bodies. Later receiver
/// specializations do not extend this set; doing so would make the observation state unbounded.
struct DescriptorPatternTypes<'a, 'db> {
    env: &'a ProgramEnvironment<'db>,
    types: RefCell<FxHashSet<Type<'db>>>,
    active: ActiveRecursionDetector<TypeIdentity<'db>>,
}

impl<'db> TypeVisitor<'db> for DescriptorPatternTypes<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        true
    }
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if self.types.borrow_mut().insert(ty) {
            self.active.visit(
                &ty.to_type_identity(db),
                || (),
                || {
                    if let TypeKind::NonAtomic(ty) = TypeKind::from(ty) {
                        walk_non_atomic_type(db, ty, self);
                    }
                },
            );
        }
    }
    fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
        for &ty in alias.specialization(db).types(db) {
            self.visit_type(db, ty);
        }
    }
}

impl<'db> CallableRecursionGuard<'db> {
    /// Follow a resolved callable dependency with the dispatch state that produced it.
    /// Plain forwarding helpers retain the preceding dispatch until another descriptor is invoked.
    /// Observation vocabularies live only for this path, so sibling expansions start independently.
    pub(super) fn with_dependency<R>(
        &self,
        db: &'db dyn Db,
        origin: DescriptorOrigin<'db>,
        func: impl FnOnce() -> R,
    ) -> R {
        let Some(dispatches) = origin.dispatches else {
            return func();
        };
        let previous_dispatch = self.growth.dispatch.replace(origin);
        let mut scope = DescriptorDispatchScope {
            current: &self.growth.dispatch,
            previous: previous_dispatch,
            initial_dispatches: &self.growth.initial_dispatches,
            introduced_functions: SmallVec::new(),
        };
        {
            let mut initial_dispatches = self.growth.initial_dispatches.borrow_mut();
            for dispatch in dispatches.elements(db) {
                if let Some(function) = dispatch.function(db)
                    && let Entry::Vacant(entry) = initial_dispatches.entry(function.literal(db))
                {
                    entry.insert(dispatches);
                    scope.introduced_functions.push(function.literal(db));
                }
            }
        }
        func()
    }

    /// Bounded specialization can use Salsa's cache and exact cycle recovery. Potentially growing
    /// constructors must keep the same guard across nested conversions to recognize new specializations.
    pub(super) fn for_constructor(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
    ) -> Self {
        let use_shared_cache =
            CallableDefinition::from_type(db, env, receiver, CallableExpansion::Upcast).is_none_or(
                |reference| {
                    reference.specialization.is_none()
                        || !reference.target.may_have_unbounded_specialization(db)
                },
            );
        Self {
            cache: ConstructorCallableCache {
                use_shared_cache,
                ..ConstructorCallableCache::default()
            },
            ..Self::default()
        }
    }

    pub(super) fn new(mode: CallableExpansion) -> Self {
        Self {
            mode,
            ..Self::default()
        }
    }

    pub(super) fn constructor_callables(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
        receiver: Type<'db>,
    ) -> CallableTypes<'db> {
        self.cache.get_or_insert(db, class, receiver, || {
            class.into_callable_with_recursion_guard(db, receiver, self)
        })
    }

    pub(super) fn visit<R>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: &Type<'db>,
        on_cycle: impl FnOnce() -> R,
        on_growth: impl FnOnce() -> R,
        func: impl FnOnce() -> R,
    ) -> R {
        let on_cycle = || {
            self.cache.recoveries.set(self.cache.recoveries.get() + 1);
            on_cycle()
        };
        let on_growth = || {
            self.cache.recoveries.set(self.cache.recoveries.get() + 1);
            on_growth()
        };
        if self.active.seen.borrow().contains(ty) {
            return on_cycle();
        }

        let Some(reference) = CallableDefinition::from_type(db, env, *ty, self.mode) else {
            let Some(identity) = ty.recursive_identity(db) else {
                return self.active.visit(ty, on_cycle, func);
            };
            return self.identities.visit(&identity, on_growth, || {
                self.active.visit(ty, on_cycle, func)
            });
        };

        if self.growth.should_approximate(db, env, reference) {
            return on_growth();
        }
        let dispatch_reference = (reference, self.growth.dispatch.get());
        if self
            .growth
            .active
            .seen
            .borrow()
            .contains(&dispatch_reference)
        {
            return self.active.visit(ty, on_cycle, func);
        }

        self.growth
            .active
            .visit(&dispatch_reference, on_growth, || {
                self.active.visit(ty, on_cycle, func)
            })
    }
}

struct DescriptorDispatchScope<'a, 'db> {
    current: &'a Cell<DescriptorOrigin<'db>>,
    previous: DescriptorOrigin<'db>,
    initial_dispatches: &'a RefCell<FxHashMap<FunctionLiteral<'db>, DescriptorDispatches<'db>>>,
    introduced_functions: SmallVec<[FunctionLiteral<'db>; 1]>,
}

impl Drop for DescriptorDispatchScope<'_, '_> {
    fn drop(&mut self) {
        self.current.set(self.previous);
        let mut initial_dispatches = self.initial_dispatches.borrow_mut();
        for function in &self.introduced_functions {
            initial_dispatches.remove(function);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::{
        CycleDetector, CycleDetectorVisit, Db, FlowEdge, FlowKind, HasIdentity,
        RecursiveDefinition, SpecializationFlowGraph, TypeIdentity,
    };
    use crate::ProgramEnvironment;
    use crate::db::tests::setup_db;
    use crate::place::global_symbol;
    use crate::types::{KnownInstanceType, Type, TypeAliasType};
    use ruff_db::files::system_path_to_file;
    use ruff_db::system::DbWithWritableSystem;
    use std::cell::Cell;
    use std::hash::{Hash, Hasher};
    use ty_python_core::ProgramFile;

    struct TestVisit;

    type Detector<'db> = CycleDetector<'db, TestVisit, u8, u8, 1>;

    impl<'db> HasIdentity<'db> for u8 {
        type Id = Self;

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
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

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
            self.identity_calls.set(self.identity_calls.get() + 1);
            self.value
        }
    }

    #[derive(Clone, Debug, Eq, Hash, PartialEq)]
    struct ConstantIdentityItem(u8);

    impl<'db> HasIdentity<'db> for ConstantIdentityItem {
        type Id = ();

        fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {}
    }

    fn global_instance_type<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
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
        env: &ProgramEnvironment<'db>,
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
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(detector.visit(db, 1, || 10), 10);
        assert_eq!(detector.visit(db, 1, || 40), 10);
        assert_eq!(detector.visit(db, 2, || 20), 20);
        assert!(!detector.cache.borrow().is_spilled());
        assert_eq!(detector.visit(db, 3, || 30), 30);
        assert!(detector.cache.borrow().is_spilled());

        assert_eq!(detector.visit(db, 2, || 40), 20);
        assert_eq!(detector.visit(db, 3, || 40), 30);
    }

    #[test]
    fn nested_visit_short_circuits_on_cycle() {
        let db = setup_db();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(
            detector.visit(db, 1, || detector.visit(db, 1, || 20) + 10),
            10
        );
    }

    #[test]
    fn selectively_reuses_cached_results() {
        let db = setup_db();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(detector.try_visit(db, 1, |_| true, || 10), Ok(10));
        assert_eq!(
            detector.try_visit(db, 1, |&result| result == 10, || 20),
            Ok(10)
        );
        assert_eq!(
            detector.try_visit(
                db,
                1,
                |&result| result == 20,
                || {
                    assert_eq!(detector.try_visit(db, 1, |_| false, || 30), Ok(0));
                    detector.visit(db, 1, || 30) + 10
                }
            ),
            Ok(20)
        );
        assert_eq!(detector.visit(db, 1, || 30), 10);
    }

    #[test]
    fn recomputed_visits_share_exact_recursion_guards() {
        let db = setup_db();
        let db = &db;
        let detector = Detector::new(0);

        assert_eq!(
            detector.try_visit(db, 1, |_| false, || detector.visit(db, 1, || 20) + 10),
            Ok(10)
        );
        assert_eq!(
            detector.visit(db, 2, || {
                assert_eq!(detector.try_visit(db, 2, |_| false, || 20), Ok(0));
                10
            }),
            10
        );
    }

    #[test]
    fn recomputed_visits_share_abstract_identity_guards() {
        let db = setup_db();
        let db = &db;
        let detector = CycleDetector::<TestVisit, ConstantIdentityItem, u8, 1>::new(0);

        assert_eq!(
            detector.try_visit(
                db,
                ConstantIdentityItem(1),
                |_| false,
                || {
                    assert_eq!(
                        detector.try_visit(db, ConstantIdentityItem(2), |_| true, || 20),
                        Err(ConstantIdentityItem(2))
                    );
                    10
                }
            ),
            Ok(10)
        );
        assert_eq!(
            detector.try_visit(
                db,
                ConstantIdentityItem(3),
                |_| true,
                || {
                    assert_eq!(
                        detector.try_visit(db, ConstantIdentityItem(4), |_| false, || 20),
                        Err(ConstantIdentityItem(4))
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
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || {
                detector.visit(db, CountingIdentityItem::new(3, &identity_calls), || 1)
            }),
            1
        );
        assert_eq!(identity_calls.get(), 2);
    }

    #[test]
    fn skips_identity_for_distinct_candidates() {
        let db = setup_db();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || {
                detector.visit(db, CountingIdentityItem::new(2, &identity_calls), || 1)
            }),
            1
        );
        assert_eq!(identity_calls.get(), 0);
    }

    #[test]
    fn skips_identity_without_a_distinct_active_item() {
        let db = setup_db();
        let db = &db;
        let identity_calls = Cell::new(0);
        let detector = CycleDetector::<TestVisit, CountingIdentityItem<'_>, u8, 1>::new(0);

        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || 1),
            1
        );
        assert_eq!(
            detector.visit(db, CountingIdentityItem::new(1, &identity_calls), || 2),
            1
        );
        assert_eq!(identity_calls.get(), 0);
    }

    #[test]
    fn different_items_with_same_identity_form_cycle() {
        let db = setup_db();
        let db = &db;
        let detector = CycleDetector::<TestVisit, ConstantIdentityItem, u8, 1>::new(0);

        let CycleDetectorVisit::Pending(pending) =
            detector.begin_visit(db, ConstantIdentityItem(1))
        else {
            panic!("the first identity should be pending");
        };
        let CycleDetectorVisit::Cycle(item) = detector.begin_visit(db, ConstantIdentityItem(2))
        else {
            panic!("a different item with the same identity should form a cycle");
        };
        assert_eq!(item.0, 2);
        detector.finish_visit(pending, 1);

        let CycleDetectorVisit::Ready(seen) = detector.begin_visit(db, ConstantIdentityItem(1))
        else {
            panic!("the first identity should be ready after the pending visit is finished");
        };
        assert_eq!(seen, 1);
        let CycleDetectorVisit::Pending(pending) =
            detector.begin_visit(db, ConstantIdentityItem(2))
        else {
            panic!("the second identity should be pending after the first is finished");
        };
        detector.finish_visit(pending, 2);
        let CycleDetectorVisit::Ready(seen) = detector.begin_visit(db, ConstantIdentityItem(2))
        else {
            panic!("the second identity should be ready after the pending visit is finished");
        };
        assert_eq!(seen, 2);
    }
}
