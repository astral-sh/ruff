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
//! graph index and the specialization `T := list[T]` in its mapping sequence.
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

use super::callable::walk_callable_type;
use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::function::walk_function_type;
use super::generics::{ApplySpecialization, Specialization};
use super::instance::walk_protocol_instance_type;
use super::mapping::DeferredTypeMapping;
use super::relation::{TypeRelation, TypeRelationChecker};
use super::type_alias::{AliasCycleSummary, walk_type_alias_arguments};
use super::typed_dict::walk_typed_dict_type;
use super::variance::{VarianceInferable, VarianceOrigin};
use super::visitor::{
    TypeCollector, TypeVisitor, any_over_type_including_alias_arguments,
    walk_type_with_recursion_guard,
};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, CallableType,
    FunctionType, GenericContext, MaterializationKind, ProtocolInstanceType, Type, TypeAliasType,
    TypeContext, TypeMapping, TypeVarInstance, TypedDictType, VarianceTerm,
};
use crate::{Db, Program, ProgramEnvironment};

/// A reference to a constructor in the enclosing recursive graph.
/// An escaping reference has no type semantics; in particular, it is neither a
/// gradual type nor an assignability operand.
/// Only recursive-type binding and mapping operations may construct or operate on recursive variables.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveVar<'db> {
    /// The constructor's index in the enclosing graph. Graph-local references keep mutually
    /// recursive bodies shared during unfolding, avoiding substitution through nested binders.
    #[returns(copy)]
    index: usize,
    #[returns(ref)]
    mappings: Box<[DeferredTypeMapping<'db>]>,
}

impl get_size2::GetSize for RecursiveVar<'_> {}

impl<'db> RecursiveVar<'db> {
    /// Resolve or remap graph references while preserving their mapping sequence.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let mut mappings = self.mappings(db).to_vec();
        if mapping.is_structural() {
            for retained in &mut mappings {
                *retained = retained.rewrite(db, mapping, visitor);
            }
        }
        match mapping {
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Unfold(recursive),
            )) => Type::Recursive(RecursiveType::new_internal(
                db,
                recursive.graph(db),
                self.index(db),
                mappings.into_boxed_slice(),
            )),
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Import(builder, graph),
            )) => builder.reference(db, visitor.env, *graph, self.index(db), &mappings),
            _ => {
                for mapping in DeferredTypeMapping::from_mapping(db, mapping, tcx, visitor) {
                    mapping.append(db, &mut mappings, visitor);
                }
                Type::RecursiveVar(Self::new_internal(
                    db,
                    self.index(db),
                    mappings.into_boxed_slice(),
                ))
            }
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
    /// Replace closed applications with common variables when comparing two unfoldings.
    Fold(
        &'a RecursiveTypeComparison<'db>,
        &'a RefCell<Vec<RecursiveType<'db>>>,
    ),
    /// Simplify applications exposed by an unfolding without unfolding their bodies again.
    Simplify,
}

/// An unspecialized constructor body and its originating alias query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct RecursiveTypeNode<'db> {
    definition: Definition<'db>,
    parameters: Option<GenericContext<'db>>,
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
                    && node.parameters == other.parameters
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
        mappings: &[DeferredTypeMapping<'db>],
    ) -> Type<'db> {
        let node = graph.nodes(db)[index];
        let target = if node.cycle == self.cycle {
            self.referenced.set(true);
            0
        } else {
            self.import(db, env, graph, index)
        };
        Type::RecursiveVar(RecursiveVar::new_internal(db, target, mappings))
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
/// The private body remains unspecialized. Unfolding closes its recursive references,
/// then applies the retained specializations and other mappings in order.
/// Use the binding operations in this module to construct recursive types.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// Shared unspecialized bodies and their formal parameters.
    #[returns(copy)]
    graph: RecursiveTypeGraph<'db>,
    /// Selects a body in `graph`. Sharing the graph and indexing its bodies avoids rebuilding
    /// nested binders for mutually recursive types when unfolding a reference.
    #[returns(copy)]
    index: usize,
    /// Transformations applied, in order, after closing one body layer.
    #[returns(ref)]
    mappings: Box<[DeferredTypeMapping<'db>]>,
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
            variable.mappings(db),
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
        let graph = RecursiveTypeGraph::new(
            db,
            vec![RecursiveTypeNode {
                definition,
                parameters,
                cycle,
                iteration: 0,
                body: Type::RecursiveVar(RecursiveVar::new_internal(db, 0, Box::default())),
            }]
            .into_boxed_slice(),
        );
        Self::new_internal(db, graph, 0, Box::default())
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
            && current.mappings(db) == previous.mappings(db)
            && current.graph(db).has_same_bodies(db, previous.graph(db))
        {
            return Type::Recursive(previous);
        }
        result
    }

    /// Bind references to this alias's query cycle in a closed inference result.
    /// Each bound occurrence retains its mappings and refers to the graph's root node.
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
            Type::Recursive(Self::new_internal(db, graph, 0, Box::default()))
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

    fn with_mappings(self, db: &'db dyn Db, mappings: &[DeferredTypeMapping<'db>]) -> Self {
        Self::new_internal(db, self.graph(db), self.index(db), mappings)
    }

    pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match self.mappings(db).last()?.type_mapping() {
            TypeMapping::Materialize(kind) => Some(kind),
            _ => None,
        }
    }

    fn without_materialization(self, db: &'db dyn Db) -> Self {
        let mappings = self.mappings(db);
        if self.materialization_kind(db).is_some() {
            self.with_mappings(db, &mappings[..mappings.len() - 1])
        } else {
            self
        }
    }

    /// Parameters bound by this recursive type constructor.
    pub(super) fn parameters(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.graph(db).nodes(db)[self.index(db)].parameters
    }

    /// Project substitutions for argument discovery and display; this does not reorder execution.
    #[salsa::tracked(returns(copy), cycle_initial=|db, _, recursive: RecursiveType<'db>| recursive.parameters(db).map(|parameters| parameters.identity_specialization(db)))]
    pub(super) fn arguments(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        let mut arguments = self.parameters(db)?.identity_specialization(db);
        let env = self.environment(db);
        let visitor = ApplyTypeMappingVisitor::new(&env);
        for mapping in self
            .mappings(db)
            .iter()
            .filter(|mapping| mapping.is_substitution())
        {
            arguments =
                arguments.apply_type_mapping_impl(db, &mapping.type_mapping(), &[], &visitor);
        }
        Some(arguments)
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

    /// Restore the formal arguments and remove deferred mappings for constructor analysis.
    pub(super) fn constructor(self, db: &'db dyn Db) -> Self {
        // Like an unspecialized PEP 695 alias, parameter-flow analysis must not
        // re-enter materialization while deriving the constructor's identity.
        self.with_mappings(db, &[])
    }

    /// The program in which the recursive type's body was constructed.
    pub fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        ProgramEnvironment::from_definition(self.definition(db))
    }

    /// Substitute closed types for references before exposing the body.
    ///
    /// Report whether unfolding returns exactly `Type::Recursive(self)`. An unfolded
    /// type can still contain recursive references, so callers must retain their recursion guards.
    pub fn unfold(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> UnfoldResult<'db> {
        UnfoldResult::new(self, self.unfolding(db, env.program(db)))
    }

    // Normalization can unfold the same application without an enclosing mapping visitor.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, recursive: RecursiveType<'db>, _| Type::Recursive(recursive),
        heap_size=ruff_memory_usage::heap_size
    )]
    fn unfolding(self, db: &'db dyn Db, program: Program<'db>) -> Type<'db> {
        let env = ProgramEnvironment::from_program(program);
        self.unfold_impl(db, &ApplyTypeMappingVisitor::new(&env))
            .into_type()
    }

    /// Preserve the enclosing materialization's equivalence guards across an unfolding.
    pub(super) fn unfold_impl(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> UnfoldResult<'db> {
        let unfolded = self.mapped_body(db, visitor).apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Simplify,
            )),
            TypeContext::default(),
            visitor,
        );
        UnfoldResult::new(self, unfolded)
    }

    fn mapped_body(self, db: &'db dyn Db, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Type<'db> {
        let visitor = ApplyTypeMappingVisitor {
            recursive_unfolding: true,
            ..visitor.for_new_materialization_root()
        };
        self.mappings(db)
            .iter()
            .fold(self.closed_body(db), |ty, mapping| {
                mapping.apply(db, ty, &visitor)
            })
    }

    /// Collect specialization targets together, without unfolding growing applications.
    #[salsa::tracked(
        returns(ref),
        cycle_initial=|_, _, _| SpecializationTargets::default(),
        heap_size=ruff_memory_usage::heap_size
    )]
    fn specialization_targets(self, db: &'db dyn Db) -> SpecializationTargets<'db> {
        let Some((last, prefix)) = self.mappings(db).split_last() else {
            let mut targets = RecursiveSpecializationSearch::collect(db, self, self.body(db));
            if let Some(parameters) = self.parameters(db) {
                for variable in parameters.variables(db) {
                    targets.insert(db, variable);
                }
            }
            targets.typevars.shrink_to_fit();
            return targets;
        };
        let prefix = self.with_mappings(db, prefix);
        let Some(substitution) = last.substitution() else {
            // Other transformations can expose new targets in lazily inferred attributes.
            let mut targets = SpecializationTargets::all();
            if matches!(
                last.type_mapping(),
                TypeMapping::ApplySpecialization(ApplySpecialization::Effects { .. })
            ) {
                targets.effects = prefix.specialization_targets(db).effects;
            }
            return targets;
        };
        let input = prefix.specialization_targets(db);
        let mut targets = input.clone();
        for parameter in substitution.generic_context(db).variables(db) {
            targets.remove(parameter.identity(db));
        }
        for (parameter, ty) in substitution
            .generic_context(db)
            .variables(db)
            .zip(substitution.types(db))
        {
            let value = RecursiveSpecializationSearch::collect(db, self, *ty);
            // A substituted value contributes variables only if its parameter occurs in the input.
            // Keep possible callable/Self effects even when the parameter is unused.
            targets.effects |= value.effects;
            if input.contains(parameter.identity(db)) {
                targets.extend(&value);
            }
        }
        targets.typevars.shrink_to_fit();
        targets
    }

    /// Substitute free variables and graph references, or defer transformations until unfolding.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        match mapping {
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(substitution)) => {
                if let RecursiveSubstitution::Fold(comparison, references) = substitution {
                    return comparison.fold(db, self, references);
                }
                if let RecursiveSubstitution::Simplify = substitution {
                    return Type::Recursive(self.simplified(db));
                }
                let mappings = self
                    .mappings(db)
                    .iter()
                    .map(|retained| retained.rewrite(db, mapping, visitor))
                    .collect::<Vec<_>>();
                match substitution {
                    RecursiveSubstitution::Unfold(_) => {
                        Type::Recursive(self.with_mappings(db, &mappings))
                    }
                    RecursiveSubstitution::Bind(builder)
                    | RecursiveSubstitution::Import(builder, _) => builder.reference(
                        db,
                        visitor.env,
                        self.graph(db),
                        self.index(db),
                        &mappings,
                    ),
                    RecursiveSubstitution::Fold(comparison, references) => {
                        comparison.fold(db, self, references)
                    }
                    RecursiveSubstitution::Simplify => Type::Recursive(self.simplified(db)),
                }
            }
            _ => {
                let mut mappings = self.mappings(db).to_vec();
                for mapping in DeferredTypeMapping::from_mapping(db, mapping, tcx, visitor) {
                    mapping.append(db, &mut mappings, visitor);
                }
                let mapped = self.with_mappings(db, &mappings);
                if visitor.recursive_unfolding {
                    return Type::Recursive(mapped);
                }
                Type::Recursive(mapped.simplified(db))
            }
        }
    }

    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, recursive: RecursiveType<'db>| recursive,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn simplified(self, db: &'db dyn Db) -> Self {
        let Some((last, prefix)) = self.mappings(db).split_last() else {
            return self;
        };
        // Unfolding prepends the recursive edge's mappings. Normalize that prefix before
        // comparing the final operation, so redundant cycles inside the sequence disappear.
        let prefix = self.with_mappings(db, prefix).simplified(db);
        if matches!(
            last.type_mapping(),
            TypeMapping::ApplySpecialization(ApplySpecialization::Effects { .. })
        ) && !prefix.specialization_targets(db).effects
        {
            return prefix;
        }
        let mut mappings = prefix.mappings(db).to_vec();
        let env = self.environment(db);
        let visitor = ApplyTypeMappingVisitor::new(&env);
        last.clone().append(db, &mut mappings, &visitor);
        let mut mapped = self.with_mappings(db, &mappings);
        // Composition retains both domains. Drop a binding only after checking that the
        // constructor does not use it, including through captured variables in its body.
        if let Some(last) = mappings.last()
            && let Some(substitution) = last.substitution()
        {
            let input = mapped.with_mappings(db, &mappings[..mappings.len() - 1]);
            for (index, variable) in substitution
                .generic_context(db)
                .variables(db)
                .enumerate()
                .rev()
            {
                let mut candidate_mappings = mappings.clone();
                let Some(last) = candidate_mappings.pop() else {
                    break;
                };
                if let Some(reduced) = last.without_variable(db, index, &visitor) {
                    candidate_mappings.push(reduced);
                }
                let candidate = self.with_mappings(db, &candidate_mappings);
                if !input
                    .specialization_targets(db)
                    .contains(variable.identity(db))
                    || RecursiveTypeComparison::default().equivalent(db, mapped, candidate)
                {
                    mappings = candidate_mappings;
                    mapped = candidate;
                }
            }
        }
        for prefix in 0..mappings.len() {
            let candidate = self.with_mappings(db, &mappings[..prefix]);
            if RecursiveTypeComparison::default().equivalent(db, mapped, candidate) {
                return candidate;
            }
        }
        mapped
    }

    /// For the same constructor, prove equality on every variable the shared input can expose.
    fn has_equivalent_substitution(self, db: &'db dyn Db, other: Self) -> bool {
        let left = self.mappings(db);
        let right = other.mappings(db);
        let common = left
            .iter()
            .zip(right)
            .take_while(|(left, right)| left == right)
            .count();
        let left = &left[common..];
        let right = &right[common..];
        let (left, right) = match (left, right) {
            ([left], [right])
                if left.has_same_context(right)
                    && let Some(left) = left.substitution()
                    && let Some(right) = right.substitution() =>
            {
                (Some(left), Some(right))
            }
            ([left], []) if let Some(left) = left.substitution() => (Some(left), None),
            ([], [right]) if let Some(right) = right.substitution() => (None, Some(right)),
            _ => return false,
        };
        let input = self.with_mappings(db, &self.mappings(db)[..common]);
        left.into_iter()
            .chain(right)
            .flat_map(|substitution| substitution.generic_context(db).variables(db))
            .all(|variable| {
                let left = left.and_then(|substitution| substitution.get(db, variable));
                let right = right.and_then(|substitution| substitution.get(db, variable));
                left == right
                    || input.specialization_targets(db).substitutions_agree(
                        variable.identity(db),
                        left,
                        right,
                    )
            })
    }

    pub(in crate::types) fn variance_equation(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = self.environment(db);
        self.unfold(db, &env)
            .map(|unfolded| unfolded.variance_of(db, &env, typevar))
            .unwrap_or(VarianceTerm::BIVARIANT)
    }
}

/// Possible variable occurrences and callable/Self effects of specialization.
#[derive(Clone, Debug, Default, Eq, PartialEq, salsa::SalsaValue, get_size2::GetSize)]
struct SpecializationTargets<'db> {
    // Keep a unique occurrence when known: identities omit materialized bounds and constraints.
    typevars: FxHashMap<BoundTypeVarIdentity<'db>, Option<BoundTypeVarInstance<'db>>>,
    /// Unknown attributes may contain any variable. In that case `typevars` lists exclusions,
    /// so substitution can still remove variables from an otherwise unknown set.
    complement: bool,
    effects: bool,
}

impl<'db> SpecializationTargets<'db> {
    fn all() -> Self {
        Self {
            complement: true,
            effects: true,
            ..Self::default()
        }
    }

    fn contains(&self, variable: BoundTypeVarIdentity<'db>) -> bool {
        self.typevars.contains_key(&variable) != self.complement
    }

    /// Compare replacements; a missing replacement preserves the original occurrence.
    fn substitutions_agree(
        &self,
        variable: BoundTypeVarIdentity<'db>,
        left: Option<Type<'db>>,
        right: Option<Type<'db>>,
    ) -> bool {
        if !self.contains(variable) {
            return true;
        }
        let unchanged = self
            .typevars
            .get(&variable)
            .copied()
            .flatten()
            .map(Type::TypeVar);
        left.or(unchanged) == right.or(unchanged)
    }

    fn insert(&mut self, db: &'db dyn Db, variable: BoundTypeVarInstance<'db>) {
        let occurrence_identity = variable.identity(db);
        let identity = if occurrence_identity.is_paramspec(db) {
            occurrence_identity.without_paramspec_attr(db)
        } else {
            occurrence_identity
        };
        // Do not treat a ParamSpec projection as the original occurrence of its lookup key.
        let occurrence = (identity == occurrence_identity).then_some(variable);
        if self.complement {
            self.typevars.remove(&identity);
        } else {
            let previous = self.typevars.entry(identity).or_insert(occurrence);
            if *previous != occurrence {
                *previous = None;
            }
        }
    }

    fn remove(&mut self, variable: BoundTypeVarIdentity<'db>) {
        if self.complement {
            self.typevars.insert(variable, None);
        } else {
            self.typevars.remove(&variable);
        }
    }

    fn extend(&mut self, other: &Self) {
        match (self.complement, other.complement) {
            (false, false) =>
            {
                #[expect(
                    clippy::iter_over_hash_type,
                    reason = "Merging occurrences is order-independent"
                )]
                for (identity, variable) in &other.typevars {
                    let previous = self.typevars.entry(*identity).or_insert(*variable);
                    if previous != variable {
                        *previous = None;
                    }
                }
            }
            (true, false) => self
                .typevars
                .retain(|variable, _| !other.typevars.contains_key(variable)),
            (false, true) => {
                let mut excluded = other.typevars.clone();
                excluded.retain(|variable, _| !self.typevars.contains_key(variable));
                self.typevars = excluded;
                self.complement = true;
            }
            (true, true) => self
                .typevars
                .retain(|variable, _| other.typevars.contains_key(variable)),
        }
        self.effects |= other.effects;
    }
}

/// Search open graph bodies and substitution values, preserving the scope of recursive variables.
struct RecursiveSpecializationSearch<'db> {
    scope: RecursiveType<'db>,
    env: ProgramEnvironment<'db>,
    targets: RefCell<SpecializationTargets<'db>>,
    visited: TypeCollector<'db>,
}

impl<'db> RecursiveSpecializationSearch<'db> {
    fn collect(
        db: &'db dyn Db,
        scope: RecursiveType<'db>,
        ty: Type<'db>,
    ) -> SpecializationTargets<'db> {
        let search = Self {
            scope,
            env: scope.environment(db),
            targets: RefCell::default(),
            visited: TypeCollector::default(),
        };
        search.visit_type(db, ty);
        search.targets.into_inner()
    }
}

impl<'db> TypeVisitor<'db> for RecursiveSpecializationSearch<'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        &self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn notify_skipped_lazy_type_attributes(&self) {
        *self.targets.borrow_mut() = SpecializationTargets::all();
    }

    fn visit_parameter_default(&self, db: &'db dyn Db, ty: Type<'db>) {
        self.visit_type(db, ty);
    }

    // Substitution preserves generic declarations and TypeVar objects. Actual variable
    // occurrences are handled in visit_type, without inspecting their bounds or defaults.
    fn visit_type_var_type(&self, _db: &'db dyn Db, _typevar: TypeVarInstance<'db>) {}

    // Specialization maps stored class arguments, not lazily inferred members or fields.
    fn visit_protocol_instance_type(&self, db: &'db dyn Db, protocol: ProtocolInstanceType<'db>) {
        if let Some(class) = protocol.class_origin(db) {
            self.visit_type(db, class.into());
        } else {
            walk_protocol_instance_type(db, protocol, self);
        }
    }

    fn visit_typed_dict_type(&self, db: &'db dyn Db, typed_dict: TypedDictType<'db>) {
        if let TypedDictType::Class(class) = typed_dict {
            self.visit_type(db, class.into());
        } else {
            walk_typed_dict_type(db, typed_dict, self);
        }
    }

    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        walk_type_alias_arguments(db, alias, self);
    }

    // Alias objects are unchanged by specialization, unlike aliases used as types.
    fn visit_type_alias_object(&self, _db: &'db dyn Db, _alias: TypeAliasType<'db>) {}

    fn visit_callable_type(&self, db: &'db dyn Db, callable: CallableType<'db>) {
        self.targets.borrow_mut().effects = true;
        walk_callable_type(db, callable, self);
    }

    fn visit_function_type(&self, db: &'db dyn Db, function: FunctionType<'db>) {
        self.targets.borrow_mut().effects = true;
        walk_function_type(db, function, self);
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        match ty {
            Type::TypeVar(typevar) => {
                let mut targets = self.targets.borrow_mut();
                targets.insert(db, typevar);
                targets.effects |= typevar.typevar(db).is_self(db);
            }
            Type::RecursiveVar(reference) => {
                let targets = self
                    .scope
                    .reference(db, reference)
                    .specialization_targets(db);
                self.targets.borrow_mut().extend(targets);
            }
            Type::Recursive(recursive) => {
                let targets = recursive.specialization_targets(db);
                self.targets.borrow_mut().extend(targets);
            }
            _ => {
                walk_type_with_recursion_guard(db, ty, self, &self.visited);
            }
        }
    }
}

/// The outcome of unfolding one layer of a [`RecursiveType`].
///
/// Mapping transforms the unfolded value while retaining the original recursive type
/// when unfolding made no progress.
#[derive(Debug, Clone, Copy)]
#[must_use]
pub enum UnfoldResult<'db, T = Type<'db>> {
    /// The unfolded type, or a value produced by mapping it.
    Unfolded(T),
    /// Unfolding returns the original recursive type, for example for `μa. a`.
    Unchanged(RecursiveType<'db>),
}

impl<'db> UnfoldResult<'db, Type<'db>> {
    fn new(recursive: RecursiveType<'db>, unfolded: Type<'db>) -> Self {
        if unfolded == Type::Recursive(recursive) {
            Self::Unchanged(recursive)
        } else {
            Self::Unfolded(unfolded)
        }
    }

    /// Return the unfolded or mapped type, or the original recursive type if unfolding made no progress.
    ///
    /// Callers that recursively process the returned type must use their own recursion guards.
    /// Unfolding one layer does not eliminate cycles, even when it makes progress.
    #[inline]
    pub fn into_type(self) -> Type<'db> {
        match self {
            Self::Unfolded(ty) => ty,
            Self::Unchanged(recursive) => Type::Recursive(recursive),
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
        source: RecursiveType<'db>,
        target: RecursiveType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let (
            Some((source_substitution, source_mappings)),
            Some((target_substitution, target_mappings)),
        ) = (
            source
                .without_materialization(db)
                .mappings(db)
                .split_first(),
            target
                .without_materialization(db)
                .mappings(db)
                .split_first(),
        )
        else {
            return self.never();
        };
        if !matches!(
            self.relation,
            TypeRelation::Subtyping | TypeRelation::Assignability
        ) || source.constructor(db) != target.constructor(db)
            || source_mappings != target_mappings
        {
            return self.never();
        }
        // Include captured variables: equal formal arguments do not imply equal bodies.
        let (Some(source_arguments), Some(target_arguments)) = (
            source_substitution.substitution(),
            target_substitution.substitution(),
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
        let source_kind =
            source
                .materialization_kind(db)
                .unwrap_or(if self.relation.is_assignability() {
                    MaterializationKind::Bottom
                } else {
                    MaterializationKind::Top
                });
        let target_kind =
            target
                .materialization_kind(db)
                .unwrap_or(if self.relation.is_assignability() {
                    MaterializationKind::Top
                } else {
                    MaterializationKind::Bottom
                });
        // Top-to-bottom also requires a static body: equal arguments do not rule
        // out a fixed `Any`. Unchanged materializations retain the original binder.
        let unmaterialized_target = Type::Recursive(target.without_materialization(db));
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

/// Compare closed unfoldings by folding equivalent recursive references to common variables.
#[derive(Debug, Default, PartialEq, Eq)]
struct RecursiveTypeComparison<'db> {
    active: RefCell<Vec<(RecursiveType<'db>, RecursiveType<'db>)>>,
    unfolded_pairs: Cell<usize>,
}

impl get_size2::GetSize for RecursiveTypeComparison<'_> {}

impl<'db> RecursiveTypeComparison<'db> {
    fn equivalent(
        &self,
        db: &'db dyn Db,
        left: RecursiveType<'db>,
        right: RecursiveType<'db>,
    ) -> bool {
        const MAX_UNFOLDED_PAIRS: usize = 16;
        const MAX_UNFOLDING_DEPTH: usize = 4;

        if left == right {
            return true;
        }
        if left.graph(db) != right.graph(db) || left.index(db) != right.index(db) {
            return false;
        }
        // A provisional body may not expose its parameters yet. Preserve their substitutions.
        if left.arguments(db) != right.arguments(db) {
            return false;
        }
        if left.has_equivalent_substitution(db, right) {
            return true;
        }
        {
            let active = self.active.borrow();
            // Bound stack use separately from the work shared by all branches.
            if active.len() >= MAX_UNFOLDING_DEPTH
                || active.contains(&(left, right))
                || active.contains(&(right, left))
            {
                return true;
            }
        }
        // Bound the whole comparison, including repeated visits on different branches.
        // A transformation first observable beyond this budget can be treated as redundant.
        let unfolded_pairs = self.unfolded_pairs.get();
        if unfolded_pairs >= MAX_UNFOLDED_PAIRS {
            return true;
        }
        self.unfolded_pairs.set(unfolded_pairs + 1);
        self.active.borrow_mut().push((left, right));
        let env = left.environment(db);
        let references = RefCell::default();
        let mapping = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
            RecursiveSubstitution::Fold(self, &references),
        ));
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let left = left.mapped_body(db, &visitor).apply_type_mapping_impl(
            db,
            &mapping,
            TypeContext::default(),
            &visitor,
        );
        let right = right.mapped_body(db, &visitor).apply_type_mapping_impl(
            db,
            &mapping,
            TypeContext::default(),
            &visitor,
        );
        self.active.borrow_mut().pop();
        left == right
    }

    fn fold(
        &self,
        db: &'db dyn Db,
        recursive: RecursiveType<'db>,
        references: &RefCell<Vec<RecursiveType<'db>>>,
    ) -> Type<'db> {
        let candidates = references.borrow().clone();
        let index = candidates
            .iter()
            .position(|candidate| self.equivalent(db, recursive, *candidate));
        let index = index.unwrap_or_else(|| {
            let mut references = references.borrow_mut();
            let index = references.len();
            references.push(recursive);
            index
        });
        Type::RecursiveVar(RecursiveVar::new_internal(db, index, Box::default()))
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
