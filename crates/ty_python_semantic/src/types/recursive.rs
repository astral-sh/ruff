//! Structural recursive types and their binding and substitution operations.
//!
//! A structural recursive type describes recursion within the type itself.
//! We write `μa. B` for a recursive type with body `B`, where occurrences of the recursive
//! variable `a` refer to the whole type. The `μa` is called a binder: it determines
//! what `a` refers to within `B`. For example:
//!
//! ```text
//! R = μa. tuple[int, a | None]
//! ```
//!
//! Such a recursive type is constructed, for example,
//! from the implicit type alias `R = tuple[int, "R | None"]`.
//!
//! A type is *closed* if every recursive variable is bound within that type;
//! otherwise, it is *open* (there is a dangling reference).
//! All types exposed to ordinary type operations must be closed, including during
//! intermediate normalization steps. The binding and substitution operations
//! in this module must maintain that invariant: an open body cannot be passed
//! directly to operations such as assignability checking.
//!
//! Substitution replaces occurrences of a variable with a type. It is
//! *capture-avoiding* when it preserves which binder every remaining variable refers
//! to, including variables in the inserted expression. We write `B[a := R]` for this
//! substitution of `R` for `a` in `B`. Unfolding exposes one layer of a recursive type
//! by substituting the whole type for its bound references:
//!
//! ```text
//! unfold(μa. B) = B[a := μa. B]
//! unfold(R) = tuple[int, R | None]
//! ```
//!
//! The result is closed, so ordinary type operations can inspect it. Binding works in
//! the other direction: it replaces applications of a recursive type with variables.
//! Mutually recursive bodies share a `RecursiveTypeGraph`; a `RecursiveType` selects one
//! body by index, and each `RecursiveVar` refers to another index in that same graph.
//! Unfolding replaces these variables with applications sharing the original graph,
//! without copying the other bodies. Importing a body into a graph remaps its indices
//! so that references retain their binding scope. During cycle recovery, references to older
//! approximations use the newest available body for their originating alias query.
//!
//! A *type constructor* takes type arguments and produces a type. Recursive types can
//! also be parameterized this way. With type variable `T`, the alias
//! `Tree = tuple[T, "Tree[list[T]] | None"]` has the recursive type constructor:
//!
//! ```text
//! μF. λT. tuple[T, F[list[T]] | None]
//! ```
//!
//! Here `μF` binds the recursive constructor, and `λT` binds its type parameter.
//! The occurrence `F[list[T]]` is stored as `RecursiveVar` with the constructor's
//! graph index and unspecialized arguments `[list[T]]`.
//!
//! To infer `x[1]` for `x: Tree[int]`, first unfold `x`'s type: substitute the constructor
//! for `F`, then apply `T := int`. This order closes the body before specializing it:
//!
//! ```text
//! unfold((μF. λT. tuple[T, F[list[T]] | None])[int])
//! = tuple[int, (μF. λT. tuple[T, F[list[T]] | None])[list[int]] | None]
//! = tuple[int, Tree[list[int]] | None]
//! ```
//!
//! Tuple subscripting then selects the element at index 1: `x[1]: Tree[list[int]] | None`.

use std::cell::{Cell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::generics::{ApplySpecialization, Specialization};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::type_alias::AliasCycleSummary;
use super::variance::{VarianceInferable, VarianceOrigin};
use super::visitor::any_over_type_including_alias_arguments;
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, GenericContext, MappedType, MaterializationKind,
    Type, TypeContext, TypeMapping, VarianceTerm,
};
use crate::{Db, ProgramEnvironment};

/// A reference to a constructor in the enclosing recursive graph.
/// An escaping reference has no type semantics; in particular, it is neither a
/// gradual type nor an assignability operand.
/// Only recursive-type binding and substitution operations may construct or operate on recursive variables.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveVar<'db> {
    /// The constructor's index in the enclosing graph. Graph-local references keep mutually
    /// recursive bodies shared during unfolding, avoiding substitution through nested binders.
    #[returns(copy)]
    index: usize,
    /// The unspecialized arguments of this occurrence, in the alias definition's scope.
    /// For `Tree = tuple[T, "Tree[list[T]] | None"]`, these are `[list[T]]`.
    /// Unfolding substitutes the enclosing application's arguments for the type parameters.
    #[returns(copy)]
    arguments: Option<Specialization<'db>>,
}

impl get_size2::GetSize for RecursiveVar<'_> {}

impl<'db> RecursiveVar<'db> {
    /// Resolve or remap graph references while preserving their arguments.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let arguments = self
            .arguments(db)
            .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
        match mapping {
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Unfold(recursive),
            )) => Type::Recursive(RecursiveType::new_internal(
                db,
                recursive.graph(db),
                self.index(db),
                arguments,
            )),
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Import(builder, graph),
            )) => builder.reference(db, visitor.env, *graph, self.index(db), arguments),
            _ => unreachable!("semantic operation on an unbound recursive variable"),
        }
    }
}

/// A structural substitution that only the recursive-type binder can construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub struct RecursiveMapping<'a, 'db>(RecursiveSubstitution<'a, 'db>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum RecursiveSubstitution<'a, 'db> {
    /// Replace graph variables with closed applications sharing the graph.
    Unfold(RecursiveType<'db>),
    /// Collect a closed inference result into a graph rooted at the recovered query.
    Bind(&'a RecursiveTypeGraphBuilder<'db>),
    /// Import an open body, resolving its references in the source graph.
    Import(&'a RecursiveTypeGraphBuilder<'db>, RecursiveTypeGraph<'db>),
}

/// An unspecialized constructor body and its originating alias query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct RecursiveTypeNode<'db> {
    definition: Definition<'db>,
    cycle: RecursiveCycle,
    /// The originating query's recovery iteration; zero denotes its initial seed.
    /// Compare only nodes from the same query.
    iteration: u32,
    body: Type<'db>,
}

/// Bodies in a recursive component share one binding scope.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveTypeGraph<'db> {
    #[returns(ref)]
    nodes: Box<[RecursiveTypeNode<'db>]>,
}

impl get_size2::GetSize for RecursiveTypeGraph<'_> {}

impl<'db> RecursiveTypeGraph<'db> {
    /// Compare the ordered bodies without the iteration metadata used to select them.
    fn has_same_bodies(self, db: &'db dyn Db, other: Self) -> bool {
        let nodes = self.nodes(db);
        let other_nodes = other.nodes(db);
        nodes.len() == other_nodes.len()
            && nodes.iter().zip(other_nodes).all(|(node, other)| {
                node.definition == other.definition
                    && node.cycle == other.cycle
                    && node.body == other.body
            })
    }
}

/// Collect the newest available body for each query, remapping references in its source scope.
#[derive(Debug, PartialEq, Eq)]
struct RecursiveTypeGraphBuilder<'db> {
    cycle: RecursiveCycle,
    referenced: Cell<bool>,
    nodes: RefCell<Vec<RecursiveTypeNode<'db>>>,
    imported: RefCell<FxHashMap<RecursiveCycle, usize>>,
    latest: FxHashMap<RecursiveCycle, (RecursiveTypeGraph<'db>, usize)>,
}

impl get_size2::GetSize for RecursiveTypeGraphBuilder<'_> {}

impl<'db> RecursiveTypeGraphBuilder<'db> {
    fn reference(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        graph: RecursiveTypeGraph<'db>,
        index: usize,
        arguments: Option<Specialization<'db>>,
    ) -> Type<'db> {
        let node = graph.nodes(db)[index];
        let target = if node.cycle == self.cycle {
            self.referenced.set(true);
            0
        } else {
            self.import(db, env, graph, index)
        };
        Type::RecursiveVar(RecursiveVar::new_internal(db, target, arguments))
    }

    fn import(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        graph: RecursiveTypeGraph<'db>,
        index: usize,
    ) -> usize {
        let cycle = graph.nodes(db)[index].cycle;
        if let Some(target) = self.imported.borrow().get(&cycle) {
            return *target;
        }
        let (graph, index) = self.latest.get(&cycle).copied().unwrap_or((graph, index));
        let node = graph.nodes(db)[index];
        let target = self.nodes.borrow().len();
        // Reserve the target before following backedges into the source graph.
        self.nodes.borrow_mut().push(node);
        self.imported.borrow_mut().insert(cycle, target);
        let body = node.body.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Import(self, graph),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        self.nodes.borrow_mut()[target].body = body;
        target
    }
}

/// Identifies an alias query whose provisional recursive references are being bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct RecursiveCycle(salsa::Id);

impl get_size2::GetSize for RecursiveCycle {}

/// An application of a structural recursive type constructor.
/// The private body remains unspecialized; `arguments` records this application's
/// substitution for the constructor's type parameters. Unfolding replaces recursive
/// references with closed types, then applies that substitution before exposing the
/// result to ordinary type operations.
/// Use the binding operations in this module to construct recursive types.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// Shared unspecialized bodies; specialization only changes this application's arguments.
    #[returns(copy)]
    graph: RecursiveTypeGraph<'db>,
    /// Selects a body in `graph`. Sharing the graph and indexing its bodies avoids rebuilding
    /// nested binders for mutually recursive types when unfolding a reference.
    #[returns(copy)]
    index: usize,
    /// The actual arguments for this application, which may themselves contain type variables.
    /// They are applied when unfolding; the stored body remains unspecialized.
    #[returns(copy)]
    pub(super) arguments: Option<Specialization<'db>>,
}

impl get_size2::GetSize for RecursiveType<'_> {}

#[salsa::tracked]
impl<'db> RecursiveType<'db> {
    /// The defining symbol of this constructor, including for qualified references.
    pub(super) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        self.graph(db).nodes(db)[self.index(db)].definition
    }

    /// Resolve a variable in this graph without expanding or specializing its body.
    pub(super) fn reference(self, db: &'db dyn Db, variable: RecursiveVar<'db>) -> Self {
        Self::new_internal(
            db,
            self.graph(db),
            variable.index(db),
            variable.arguments(db),
        )
    }

    fn cycle(self, db: &'db dyn Db) -> RecursiveCycle {
        self.graph(db).nodes(db)[self.index(db)].cycle
    }

    fn body(self, db: &'db dyn Db) -> Type<'db> {
        self.graph(db).nodes(db)[self.index(db)].body
    }

    fn closed_body(self, db: &'db dyn Db) -> Type<'db> {
        self.body(db).apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Unfold(self),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&self.environment(db)),
        )
    }

    /// Summarize the open constructor body without applying semantic substitutions.
    pub(super) fn cycle_summary(self, db: &'db dyn Db) -> &'db AliasCycleSummary<'db> {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|db, _, recursive: RecursiveType<'db>, ()| AliasCycleSummary::from_type(db, Type::divergent_alias(recursive.cycle(db).0), None),
            heap_size=ruff_memory_usage::heap_size
        )]
        fn cycle_summary_impl<'db>(
            db: &'db dyn Db,
            recursive: RecursiveType<'db>,
            (): (),
        ) -> AliasCycleSummary<'db> {
            AliasCycleSummary::from_type(db, recursive.body(db), Some(recursive))
        }

        cycle_summary_impl(db, self.constructor(db), ())
    }

    /// Seed a query cycle with a one-node graph representing `μa. a`.
    pub(super) fn initial(
        db: &'db dyn Db,
        definition: Definition<'db>,
        cycle: salsa::Id,
        parameters: Option<GenericContext<'db>>,
    ) -> Self {
        let cycle = RecursiveCycle(cycle);
        let arguments = parameters.map(|parameters| parameters.identity_specialization(db));
        let graph = RecursiveTypeGraph::new(
            db,
            vec![RecursiveTypeNode {
                definition,
                cycle,
                iteration: 0,
                body: Type::RecursiveVar(RecursiveVar::new_internal(db, 0, arguments)),
            }]
            .into_boxed_slice(),
        );
        Self::new_internal(db, graph, 0, arguments)
    }

    /// Close recursive occurrences after inferring an alias's constructor expression.
    pub(super) fn recover(
        db: &'db dyn Db,
        definition: Definition<'db>,
        cycle: &salsa::Cycle<'_>,
        parameters: Option<GenericContext<'db>>,
        result: Type<'db>,
        previous: Type<'db>,
    ) -> Type<'db> {
        // Shared dependencies can still contain older iterations of this alias. They refer
        // to the same definition even when their provisional bodies differ. Derive the binder
        // from the query inputs: an earlier result might not expose recursive references yet.
        let result = Self::initial(db, definition, cycle.id(), parameters).bind(
            db,
            &ProgramEnvironment::from_definition(definition),
            result,
            cycle.iteration() + 1,
        );
        // Iteration metadata selects bodies but must not keep an unchanged type from converging.
        if let (Type::Recursive(current), Type::Recursive(previous)) = (result, previous)
            && current.index(db) == previous.index(db)
            && current.arguments(db) == previous.arguments(db)
            && current.graph(db).has_same_bodies(db, previous.graph(db))
        {
            return Type::Recursive(previous);
        }
        result
    }

    /// Bind references to this alias's query cycle in a closed inference result.
    /// Each bound occurrence retains its arguments and refers to the graph's root node.
    fn bind(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original: Type<'db>,
        iteration: u32,
    ) -> Type<'db> {
        let latest =
            RefCell::new(FxHashMap::<RecursiveCycle, (RecursiveTypeGraph<'db>, usize)>::default());
        let seen = RefCell::new(FxHashSet::default());
        // Select bodies before following edges: a newer occurrence can appear after an older
        // one, and redirecting those edges during import must not depend on traversal order.
        any_over_type_including_alias_arguments(db, env, original, |nested| {
            if let Type::Recursive(recursive) = nested
                && seen.borrow_mut().insert(recursive.graph(db))
            {
                let graph = recursive.graph(db);
                for (index, node) in graph.nodes(db).iter().enumerate() {
                    let mut latest = latest.borrow_mut();
                    let entry = latest.entry(node.cycle).or_insert((graph, index));
                    if node.iteration > entry.0.nodes(db)[entry.1].iteration {
                        *entry = (graph, index);
                    }
                }
            }
            false
        });
        let builder = RecursiveTypeGraphBuilder {
            cycle: self.cycle(db),
            referenced: Cell::new(false),
            nodes: RefCell::new(vec![self.graph(db).nodes(db)[self.index(db)]]),
            imported: RefCell::default(),
            latest: latest.into_inner(),
        };
        let body = original.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Bind(&builder),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        let result = if builder.referenced.get() {
            let mut nodes = builder.nodes.into_inner();
            nodes[0].body = body;
            nodes[0].iteration = iteration;
            let graph = RecursiveTypeGraph::new(db, nodes.into_boxed_slice());
            Type::Recursive(Self::new_internal(db, graph, 0, self.arguments(db)))
        } else {
            original
        };
        // An earlier recovery can leave an unguarded cycle even without references to bind.
        if result.has_unguarded_alias_cycle(db) {
            Type::divergent_alias(self.cycle(db).0)
        } else {
            result
        }
    }

    fn with_arguments(self, db: &'db dyn Db, arguments: Option<Specialization<'db>>) -> Self {
        Self::new_internal(db, self.graph(db), self.index(db), arguments)
    }

    /// Parameters bound by this recursive type constructor.
    pub(super) fn parameters(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.arguments(db)
            .map(|arguments| arguments.generic_context(db))
    }

    /// The declared alias name, shared by all specializations of this constructor.
    pub(super) fn name(self, db: &'db dyn Db) -> &'db str {
        let definition = self.definition(db);
        // Qualified uses retain the original declaration's symbol, not the access expression.
        place_table(db, definition.scope(db))
            .symbol(definition.place(db).expect_symbol())
            .name()
    }

    /// The source alias's definition and name, if this binder comes from an alias.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "Keep alias metadata optional for inferred recursive types"
    )]
    pub(super) fn alias(self, db: &'db dyn Db) -> Option<(Definition<'db>, &'db str)> {
        // Only legacy type alias inference constructs recursive types at present.
        Some((self.definition(db), self.name(db)))
    }

    /// Restore the formal arguments for constructor analysis.
    pub(super) fn constructor(self, db: &'db dyn Db) -> Self {
        self.with_arguments(
            db,
            self.parameters(db)
                .map(|parameters| parameters.identity_specialization(db)),
        )
    }

    /// The program in which the recursive type's body was constructed.
    pub(super) fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        ProgramEnvironment::from_definition(self.definition(db))
    }

    /// Substitute closed types for references before exposing the body.
    ///
    /// Report whether unfolding returns exactly `Type::Recursive(self)`. An unfolded
    /// type can still contain recursive references, so callers must retain their recursion guards.
    pub fn unfold(self, db: &'db dyn Db) -> UnfoldResult<'db> {
        let unfolded = self.unfolded_body(db);
        if unfolded == Type::Recursive(self) {
            UnfoldResult::Unchanged(Type::Recursive(self))
        } else {
            UnfoldResult::Unfolded(unfolded)
        }
    }

    /// Share the closed, specialized body across mappings with different visitors.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, recursive: RecursiveType<'db>| Type::Recursive(recursive),
        heap_size=ruff_memory_usage::heap_size
    )]
    fn unfolded_body(self, db: &'db dyn Db) -> Type<'db> {
        let env = self.environment(db);
        let unfolded = self.closed_body(db);
        match self.arguments(db) {
            Some(arguments) => unfolded.apply_type_mapping(
                db,
                &env,
                &TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(arguments)),
                TypeContext::default(),
            ),
            None => unfolded,
        }
    }

    /// Substitute graph references or transform this application's arguments.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        match mapping {
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(substitution)) => {
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                match substitution {
                    RecursiveSubstitution::Unfold(_) => {
                        Type::Recursive(self.with_arguments(db, arguments))
                    }
                    RecursiveSubstitution::Bind(builder)
                    | RecursiveSubstitution::Import(builder, _) => builder.reference(
                        db,
                        visitor.env,
                        self.graph(db),
                        self.index(db),
                        arguments,
                    ),
                }
            }
            TypeMapping::ApplySpecialization(_)
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::FreshenBoundTypeVars { .. }
            | TypeMapping::BindSelf(_)
            | TypeMapping::ReplaceSelf { .. } => {
                // These mappings substitute free variables, which are captured by the alias's
                // arguments. Its formal body must remain independent of the calling context.
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::Recursive(self.with_arguments(db, arguments))
            }
            TypeMapping::EagerExpansion => {
                visitor.visit(db, Type::Recursive(self), mapping, || {
                    // Expand arguments only where the body exposes them. Expanding stored arguments
                    // first can feed a recursive alias's previous approximation into its own arguments.
                    self.unfold(db)
                        .map(|unfolded| {
                            let mapped =
                                unfolded.apply_type_mapping_impl(db, mapping, tcx, visitor);
                            if mapped == unfolded {
                                Type::Recursive(self)
                            } else {
                                mapped
                            }
                        })
                        .into_type()
                })
            }
            _ => MappedType::defer(db, Type::Recursive(self), mapping, tcx, visitor)
                .unwrap_or(Type::Recursive(self)),
        }
    }

    pub(in crate::types) fn variance_equation(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = self.environment(db);
        self.unfold(db)
            .map(|unfolded| unfolded.variance_of(db, &env, typevar))
            .unwrap_or(VarianceTerm::BIVARIANT)
    }
}

/// The outcome of exposing one layer of an alias, recursive type, or mapped type.
///
/// Mapping transforms the unfolded value while retaining the original recursive type
/// when unfolding made no progress.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub enum UnfoldResult<'db, T = Type<'db>> {
    /// The unfolded type, or a value produced by mapping it.
    Unfolded(T),
    /// Unfolding returns the original recursive type, for example for `μa. a`.
    Unchanged(Type<'db>),
}

impl<'db> UnfoldResult<'db, Type<'db>> {
    /// Return the unfolded or mapped type, or the original recursive type if unfolding made no progress.
    ///
    /// Callers that recursively process the returned type must use their own recursion guards.
    /// Unfolding one layer does not eliminate cycles, even when it makes progress.
    #[inline]
    pub fn into_type(self) -> Type<'db> {
        match self {
            Self::Unfolded(ty) => ty,
            Self::Unchanged(ty) => ty,
        }
    }
}

impl<'db, T> UnfoldResult<'db, T> {
    /// Return the contained value if unfolding made progress.
    #[inline]
    pub(crate) fn into_unfolded(self) -> Option<T> {
        match self {
            Self::Unfolded(value) => Some(value),
            Self::Unchanged(_) => None,
        }
    }

    /// Return whether unfolding made progress and the contained value satisfies `predicate`.
    #[inline]
    #[expect(
        clippy::wrong_self_convention,
        reason = "Like Option::is_some_and, the predicate consumes the contained value."
    )]
    pub(crate) fn is_unfolded_and(self, predicate: impl FnOnce(T) -> bool) -> bool {
        match self {
            Self::Unfolded(value) => predicate(value),
            Self::Unchanged(_) => false,
        }
    }

    /// Return whether unfolding made no progress or the contained value satisfies `predicate`.
    #[inline]
    #[expect(
        clippy::wrong_self_convention,
        reason = "Like Option::is_none_or, the predicate consumes the contained value."
    )]
    pub(crate) fn is_unchanged_or(self, predicate: impl FnOnce(T) -> bool) -> bool {
        match self {
            Self::Unfolded(value) => predicate(value),
            Self::Unchanged(_) => true,
        }
    }

    /// Transform the contained value, preserving the original recursive type if unfolding made no progress.
    #[inline]
    pub(crate) fn map<U>(self, operation: impl FnOnce(T) -> U) -> UnfoldResult<'db, U> {
        match self {
            Self::Unfolded(value) => UnfoldResult::Unfolded(operation(value)),
            Self::Unchanged(recursive) => UnfoldResult::Unchanged(recursive),
        }
    }

    /// Return the contained value, or call `fallback` if unfolding made no progress.
    #[inline]
    pub(crate) fn unwrap_or_else(self, fallback: impl FnOnce() -> T) -> T {
        match self {
            Self::Unfolded(value) => value,
            Self::Unchanged(_) => fallback(),
        }
    }

    /// Return the contained value, or return `fallback` if unfolding made no progress.
    #[inline]
    pub(crate) fn unwrap_or(self, fallback: T) -> T {
        match self {
            Self::Unfolded(value) => value,
            Self::Unchanged(_) => fallback,
        }
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Prove a relation between applications of the same recursive constructor
    /// (same unspecialized body) based on inclusion or overlap of their type
    /// arguments' materialization families.
    ///
    /// For example, every materialization of `R[int]` is also a materialization of
    /// `R[Any]`: choosing `int` restricts the possibilities for the argument without
    /// changing the constructor's body. Comparing arguments lets us establish
    /// relations between the applications' top and bottom materializations without
    /// unfolding their bodies, even when unfolding would keep growing the arguments.
    ///
    /// Argument comparison is sufficient but not necessary to establish the relation:
    /// different arguments can produce equivalent alias types. If this check cannot
    /// establish the relation, the caller can still unfold and compare the bodies.
    pub(super) fn when_recursive_types_relate_by_arguments(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        target: Type<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let (source, source_materialization) = match source {
            Type::Mapped(mapped) if let Some(kind) = mapped.materialization_kind(db) => {
                (mapped.inner(db), Some(kind))
            }
            _ => (source, None),
        };
        let (target, target_materialization) = match target {
            Type::Mapped(mapped) if let Some(kind) = mapped.materialization_kind(db) => {
                (mapped.inner(db), Some(kind))
            }
            _ => (target, None),
        };
        let same_constructor = match (source, target) {
            (Type::Recursive(source), Type::Recursive(target)) => {
                source.constructor(db) == target.constructor(db)
            }
            (Type::TypeAlias(source), Type::TypeAlias(target)) => {
                source.unspecialized(db) == target.unspecialized(db)
            }
            _ => false,
        };
        if !matches!(
            self.relation,
            TypeRelation::Subtyping | TypeRelation::Assignability
        ) || !same_constructor
        {
            return self.never();
        }
        let (Some(source_arguments), Some(target_arguments)) = (
            source.recursive_specialization(db),
            target.recursive_specialization(db),
        ) else {
            return self.never();
        };
        // This proof substitutes plain parameter values. Variadic packs and
        // materialized specializations carry additional substitution semantics.
        if source_arguments.generic_context(db) != target_arguments.generic_context(db)
            || source_arguments.materialization_kind(db).is_some()
            || target_arguments.materialization_kind(db).is_some()
            || source_arguments.tuple(db).is_some()
            || target_arguments.tuple(db).is_some()
        {
            return self.never();
        }
        let source_kind = source_materialization.unwrap_or(if self.relation.is_assignability() {
            MaterializationKind::Bottom
        } else {
            MaterializationKind::Top
        });
        let target_kind = target_materialization.unwrap_or(if self.relation.is_assignability() {
            MaterializationKind::Top
        } else {
            MaterializationKind::Bottom
        });
        // Top-to-bottom also requires a static body: equal arguments do not rule
        // out a fixed `Any`. Unchanged materializations retain the original binder.
        let unmaterialized_target = target;
        if matches!(
            (source_kind, target_kind),
            (MaterializationKind::Top, MaterializationKind::Bottom)
        ) && unmaterialized_target.materialize(
            db,
            MaterializationKind::Top,
            self.materialization_visitor,
        ) != unmaterialized_target.materialize(
            db,
            MaterializationKind::Bottom,
            self.materialization_visitor,
        ) {
            return self.never();
        }
        source_arguments
            .types(db)
            .iter()
            .zip(target_arguments.types(db))
            .when_all(db, self.constraints, |(source, target)| {
                self.check_subtyping_in_invariant_position(
                    db,
                    *source,
                    source_kind,
                    *target,
                    target_kind,
                )
            })
    }
}

impl<'db> VarianceInferable<'db> for RecursiveType<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        VarianceTerm::variable(db, VarianceOrigin::Recursive(self), typevar)
    }
}

impl Type<'_> {
    /// Reject a bare recursive variable at a semantic-operation boundary.
    pub(super) const fn assert_not_recursive_var(self) {
        debug_assert!(
            !matches!(self, Self::RecursiveVar(_)),
            "semantic operation on an unbound recursive variable"
        );
    }
}
