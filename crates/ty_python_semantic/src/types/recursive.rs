//! Binding and capture-avoiding substitution for structural recursive types.
//!
//! `RecursiveVar` is syntax, with no standalone type semantics. Only structural
//! substitutions may inspect an open body. Ordinary type operations receive its
//! closed unfolding, including during intermediate normalization steps.

mod graph;

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashSet;
use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use self::graph::RecursiveGraphBuilder;
use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::generics::{ApplySpecialization, Specialization, walk_specialization_types};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::variance::{VarianceInferable, VarianceOrigin};
use super::visitor::{TypeKind, TypeVisitor, walk_non_atomic_type};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext,
    MaterializationKind, Type, TypeAliasType, TypeContext, TypeMapping, VarianceTerm,
};
use crate::{Db, FxIndexMap, Program, ProgramEnvironment};

/// A recursive variable, indexed by the number of intervening recursive binders.
/// Zero refers to the nearest binder. An escaping reference has no type semantics;
/// in particular, it is neither a gradual type nor an assignability operand.
/// Only binding and substitution operations may construct recursive variables.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveVar<'db> {
    /// Zero-based de Bruijn index: the number of recursive binders between this
    /// occurrence and its binder. In `μa. μb. tuple[a, b]`, `a` has index 1 and
    /// `b` has index 0. This is relative to the occurrence, not the root of the type.
    #[returns(copy)]
    depth: u32,
    /// The equation within the referenced simultaneous binder. Depth selects a
    /// binder, while this index selects one of that binder's mutually recursive types.
    #[returns(copy)]
    index: usize,
    #[returns(copy)]
    arguments: Option<Specialization<'db>>,
}

impl get_size2::GetSize for RecursiveVar<'_> {}

impl<'db> RecursiveVar<'db> {
    /// Whether this reference belongs to the innermost recursive binder.
    pub(super) fn is_innermost(self, db: &'db dyn Db) -> bool {
        self.depth(db) == 0
    }

    /// Unfold references whose depth equals the number of nested binders entered
    /// by the visitor. Smaller depths belong to inner binders and stay unchanged.
    /// Larger depths escape the closed input; binding also rejects equal depths,
    /// since its input cannot already refer to the binder being introduced.
    pub(super) fn apply_type_mapping(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let TypeMapping::Recursive(_) = mapping else {
            unreachable!("semantic operation on an unbound recursive variable");
        };
        let arguments = self
            .arguments(db)
            .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
        match mapping {
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Unfold(recursive)))
                if self.depth(db) == visitor.recursive_depth =>
            {
                recursive.at(db, self.index(db), arguments)
            }
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Reindex(indices)))
                if self.depth(db) == visitor.recursive_depth =>
            {
                Type::RecursiveVar(Self::new_internal(
                    db,
                    self.depth(db),
                    indices[self.index(db)],
                    arguments,
                ))
            }
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Rebuild(types)))
                if self.depth(db) == visitor.recursive_depth =>
            {
                debug_assert_eq!(visitor.recursive_depth, 0);
                types[self.index(db)]
            }
            TypeMapping::Recursive(_) if self.depth(db) < visitor.recursive_depth => {
                Type::RecursiveVar(Self::new_internal(
                    db,
                    self.depth(db),
                    self.index(db),
                    arguments,
                ))
            }
            _ => unreachable!("semantic operation on an unbound recursive variable"),
        }
    }
}

/// A structural substitution that only the recursive-type binder can construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub struct RecursiveMapping<'a, 'db>(RecursiveSubstitution<'a, 'db>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum RecursiveSubstitution<'a, 'db> {
    Unfold(RecursiveType<'db>),
    Bind(RecursiveType<'db>),
    Reindex(&'a [usize]),
    Rebuild(&'a [Type<'db>]),
    Extract(&'a RecursiveGraphBuilder<'db>),
}

impl<'db> RecursiveMapping<'_, 'db> {
    /// Extract closed children as graph edges. Bodies of named aliases keep their
    /// own binder; only the arguments outside that binder belong to this graph.
    pub(super) fn extract_type(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Type<'db>> {
        let RecursiveSubstitution::Extract(builder) = self.0 else {
            return None;
        };
        if visitor.recursive_depth != 0 || matches!(ty, Type::RecursiveVar(_)) {
            return Some(ty);
        }
        Some(builder.reference(db, ty))
    }
}

/// The alias query or program in which a recursive binder was constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum RecursiveOrigin<'db> {
    Alias {
        /// The original defining symbol, including for qualified references.
        definition: Definition<'db>,
        cycle: salsa::Id,
    },
    /// A closed solution of recursive type constraints, independent of any query cycle.
    ConstraintSolution(Program<'db>),
}

impl get_size2::GetSize for RecursiveOrigin<'_> {}

/// Bodies bound simultaneously. References select an equation in this group;
/// no raw body may be passed to ordinary type operations.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveGraph<'db> {
    #[returns(ref)]
    bodies: Box<[Type<'db>]>,
}

impl get_size2::GetSize for RecursiveGraph<'_> {}

/// A recursive type whose raw body is private. Unfolding substitutes closed types
/// for references before exposing the body to ordinary type operations.
/// Use the binding operations in this module to construct recursive types.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    #[returns(copy)]
    origin: RecursiveOrigin<'db>,
    #[returns(copy)]
    graph: RecursiveGraph<'db>,
    #[returns(copy)]
    entry: usize,
    /// The arguments of a closed application of this recursive constructor.
    #[returns(copy)]
    pub(super) arguments: Option<Specialization<'db>>,
    /// The lazy materialization applied to this recursive alias, if any.
    #[returns(copy)]
    pub(super) materialization_kind: Option<MaterializationKind>,
}

impl get_size2::GetSize for RecursiveType<'_> {}

impl<'db> RecursiveType<'db> {
    /// Seed a query cycle with `μa. a`: index 0 refers to the binder created here.
    pub(super) fn initial(
        db: &'db dyn Db,
        definition: Definition<'db>,
        cycle: salsa::Id,
        parameters: Option<GenericContext<'db>>,
    ) -> Type<'db> {
        let arguments = parameters.map(|parameters| parameters.identity_specialization(db));
        Type::Recursive(Self::new_internal(
            db,
            RecursiveOrigin::Alias { definition, cycle },
            RecursiveGraph::new_internal(
                db,
                vec![Type::RecursiveVar(RecursiveVar::new_internal(
                    db, 0, 0, arguments,
                ))]
                .into_boxed_slice(),
            ),
            0,
            arguments,
            None,
        ))
    }

    /// Close all equations together, preserving sharing between their solutions.
    /// Noncontractive cycles remain symbolic in the constraint solver.
    pub(super) fn from_equations(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        equations: &[(BoundTypeVarInstance<'db>, Type<'db>)],
    ) -> Option<Vec<Type<'db>>> {
        let roots: Vec<_> = equations
            .iter()
            .map(|(variable, body)| (Type::TypeVar(*variable), *body))
            .collect();
        let solution = RecursiveGraphBuilder::solve(db, env, &roots)?;
        solution.recursive.then_some(solution.types)
    }

    fn body(self, db: &'db dyn Db) -> Type<'db> {
        self.graph(db).bodies(db)[self.entry(db)]
    }

    /// Select another closed type in this binder without expanding its body.
    fn at(
        self,
        db: &'db dyn Db,
        entry: usize,
        arguments: Option<Specialization<'db>>,
    ) -> Type<'db> {
        Type::Recursive(Self::new_internal(
            db,
            self.origin(db),
            self.graph(db),
            entry,
            arguments,
            self.materialization_kind(db),
        ))
    }

    /// Choose display binders in entry order across all reachable anonymous groups.
    /// Naming multiply referenced equations also preserves sharing between components.
    pub(super) fn members(self, db: &'db dyn Db) -> Vec<Self> {
        let env = self.environment(db);
        let mut incoming = FxIndexMap::default();
        incoming.insert(self, 0);
        let mut cursor = 0;
        while let Some((&member, _)) = incoming.get_index(cursor) {
            let references = RecursiveDisplayReferences {
                env: &env,
                references: RefCell::default(),
            };
            references.visit_type(db, member.unfold(db, &env));
            for reference in references.references.into_inner() {
                *incoming.entry(reference).or_insert(0) += 1;
            }
            cursor += 1;
        }
        incoming
            .into_iter()
            .filter_map(|(member, count)| (member == self || count > 1).then_some(member))
            .collect()
    }

    /// Whether both entries belong to the same instantiated recursive graph.
    pub(super) fn shares_graph(self, db: &'db dyn Db, other: Self) -> bool {
        self.origin(db) == other.origin(db)
            && self.graph(db) == other.graph(db)
            && self.arguments(db) == other.arguments(db)
            && self.materialization_kind(db) == other.materialization_kind(db)
    }

    /// Close recursive occurrences after inferring an alias's constructor expression.
    pub(super) fn recover(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Type<'db>,
        result: Type<'db>,
    ) -> Type<'db> {
        let Type::Recursive(previous) = previous else {
            return result;
        };
        previous.bind(db, env, result)
    }

    /// Bind occurrences of this recursive constructor in a closed result.
    /// An occurrence under `d` existing binders becomes index `d` of the new outer binder.
    fn bind(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, result: Type<'db>) -> Type<'db> {
        result.assert_no_unbound_recursive_vars(db, env);
        let body = result.apply_type_mapping_impl(
            db,
            &TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Bind(self))),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        let result = self.build(db, env, body);
        result.assert_no_unbound_recursive_vars(db, env);
        result
    }

    fn build(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>, body: Type<'db>) -> Type<'db> {
        // Alias arguments can expose a reference without introducing a container.
        if body.has_unguarded_alias_cycle(db)
            && let RecursiveOrigin::Alias { cycle, .. } = self.origin(db)
        {
            return Type::divergent(cycle);
        }
        if !RecursiveReferences::contains_escaping(db, env, body) {
            return body;
        }
        Type::Recursive(Self::new_internal(
            db,
            self.origin(db),
            RecursiveGraph::new_internal(db, vec![body].into_boxed_slice()),
            0,
            self.arguments(db),
            None,
        ))
    }

    fn with_arguments(self, db: &'db dyn Db, arguments: Option<Specialization<'db>>) -> Self {
        Self::new_internal(
            db,
            self.origin(db),
            self.graph(db),
            self.entry(db),
            arguments,
            self.materialization_kind(db),
        )
    }

    fn with_materialization(
        self,
        db: &'db dyn Db,
        materialization: Option<MaterializationKind>,
    ) -> Self {
        Self::new_internal(
            db,
            self.origin(db),
            self.graph(db),
            self.entry(db),
            self.arguments(db),
            materialization,
        )
    }

    /// Parameters bound by this recursive type constructor.
    pub(super) fn parameters(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.arguments(db)
            .map(|arguments| arguments.generic_context(db))
    }

    /// The source alias's definition and name, if this binder comes from an alias.
    pub(super) fn alias(self, db: &'db dyn Db) -> Option<(Definition<'db>, &'db str)> {
        let RecursiveOrigin::Alias { definition, .. } = self.origin(db) else {
            return None;
        };
        // Qualified uses retain the original declaration's symbol.
        let name = place_table(db, definition.scope(db))
            .symbol(definition.place(db).expect_symbol())
            .name();
        Some((definition, name))
    }

    /// Restore the formal arguments and remove materialization for constructor analysis.
    pub(super) fn constructor(self, db: &'db dyn Db) -> Self {
        // Like an unspecialized PEP 695 alias, parameter-flow analysis must not
        // re-enter materialization while deriving the constructor's identity.
        self.with_materialization(db, None).with_arguments(
            db,
            self.parameters(db)
                .map(|parameters| parameters.identity_specialization(db)),
        )
    }

    /// The program in which the recursive type's body was constructed.
    pub fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        match self.origin(db) {
            RecursiveOrigin::Alias { definition, .. } => {
                ProgramEnvironment::from_definition(definition)
            }
            RecursiveOrigin::ConstraintSolution(program) => {
                ProgramEnvironment::from_program(program)
            }
        }
    }

    /// Substitute closed types for references before exposing the body.
    /// The traversal starts at depth 0 inside this binder; only nested recursive
    /// bodies increase the depth used to identify references to this binder.
    pub fn unfold(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        Type::Recursive(self).assert_no_unbound_recursive_vars(db, env);
        // A growing specialization cannot converge by repeating the same query key. Materialize
        // its closed unfolding directly, under the caller's recursion guard, instead.
        if self.materialization_kind(db).is_some() && !self.may_have_unbounded_specialization(db) {
            return materialized_unfold(db, self);
        }
        let unfolded = self.body(db).apply_type_mapping_impl(
            db,
            &TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Unfold(self))),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        unfolded.assert_no_unbound_recursive_vars(db, env);
        let unfolded = match self.arguments(db) {
            Some(arguments) => unfolded.apply_type_mapping(
                db,
                env,
                &TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(arguments)),
                TypeContext::default(),
            ),
            None => unfolded,
        };
        match self.materialization_kind(db) {
            Some(kind) => unfolded.apply_type_mapping(
                db,
                env,
                &TypeMapping::Materialize(kind),
                TypeContext::default(),
            ),
            None => unfolded,
        }
    }

    /// Structural binding replaces matching constructors with a reference whose depth
    /// equals the visitor's depth and whose index selects the graph entry. Other bodies
    /// add one binder to that depth;
    /// their application arguments use the original depth, outside their own binder.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        match mapping {
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Bind(target)))
                if self.origin(db) == target.origin(db)
                    && self.graph(db) == target.graph(db)
                    && self.materialization_kind(db) == target.materialization_kind(db) =>
            {
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    visitor.recursive_depth,
                    self.entry(db),
                    arguments,
                ))
            }
            TypeMapping::Recursive(_) => {
                let nested = visitor.with_recursive_binder();
                let bodies = self
                    .graph(db)
                    .bodies(db)
                    .iter()
                    .map(|body| body.apply_type_mapping_impl(db, mapping, tcx, &nested))
                    .collect::<Box<[_]>>();
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::Recursive(Self::new_internal(
                    db,
                    self.origin(db),
                    RecursiveGraph::new_internal(db, bodies),
                    self.entry(db),
                    arguments,
                    self.materialization_kind(db),
                ))
            }
            TypeMapping::ApplySpecialization(_)
            | TypeMapping::ApplySpecializationWithMaterialization { .. }
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::FreshenBoundTypeVars { .. }
            | TypeMapping::BindSelf(_)
            | TypeMapping::ReplaceSelf { .. }
                if matches!(self.origin(db), RecursiveOrigin::Alias { .. }) =>
            {
                // These mappings substitute free variables, which are captured by the alias's
                // arguments. Its formal body must remain independent of the calling context.
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::Recursive(self.with_arguments(db, arguments))
            }
            TypeMapping::Materialize(kind) => {
                Type::Recursive(if self.materialization_kind(db).is_some() {
                    self
                } else {
                    self.with_materialization(db, Some(*kind))
                })
            }
            _ if matches!(self.origin(db), RecursiveOrigin::Alias { .. }) => {
                // Transform the constructor before applying the arguments. Binding a specialized
                // unfolding would bake those arguments into the body of every later application.
                let constructor = self.constructor(db);
                let mapped = visitor.visit(db, Type::Recursive(constructor), mapping, || {
                    constructor.map_type(db, visitor.env, |unfolded| {
                        let mapped = unfolded.apply_type_mapping_impl(db, mapping, tcx, visitor);
                        constructor.bind(db, visitor.env, mapped)
                    })
                });
                let mapped = match self.arguments(db) {
                    Some(arguments) => mapped.apply_type_mapping(
                        db,
                        visitor.env,
                        &TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(
                            arguments.apply_type_mapping_impl(db, mapping, &[], visitor),
                        )),
                        tcx,
                    ),
                    None => mapped,
                };
                // Materialization belongs to the application: specializing a previously
                // materialized formal parameter must also materialize its replacement.
                match self.materialization_kind(db) {
                    Some(kind) => mapped.apply_type_mapping(
                        db,
                        visitor.env,
                        &TypeMapping::Materialize(kind),
                        tcx,
                    ),
                    None => mapped,
                }
            }
            _ if visitor.recursive_roots.iter().any(|root| {
                self.graph(db) == root.graph(db)
                    && self.origin(db) == root.origin(db)
                    && self.materialization_kind(db) == root.materialization_kind(db)
            }) =>
            {
                Type::Recursive(self)
            }
            _ => visitor.visit(db, Type::Recursive(self), mapping, || {
                let mut nested = visitor.fresh();
                nested.recursive_roots.push(self);
                let roots: Vec<_> = (0..self.graph(db).bodies(db).len())
                    .map(|entry| {
                        let root = Self::new_internal(
                            db,
                            self.origin(db),
                            self.graph(db),
                            entry,
                            self.arguments(db),
                            self.materialization_kind(db),
                        );
                        let mapped = root.map_type(db, visitor.env, |unfolded| {
                            unfolded.apply_type_mapping_impl(db, mapping, tcx, &nested)
                        });
                        (Type::Recursive(root), mapped)
                    })
                    .collect();
                RecursiveGraphBuilder::solve(db, visitor.env, &roots)
                    .map(|solution| solution.types[self.entry(db)])
                    .unwrap_or(Type::Recursive(self))
            }),
        }
    }

    pub(in crate::types) fn variance_equation(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = self.environment(db);
        self.map_or(db, &env, VarianceTerm::BIVARIANT, |unfolded| {
            unfolded.variance_of(db, &env, typevar)
        })
    }

    pub(crate) fn map_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        operation: impl FnOnce(Type<'db>) -> Type<'db>,
    ) -> Type<'db> {
        self.map_or_else(db, env, || Type::Recursive(self), operation)
    }

    pub(crate) fn map_or_else<F>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        fallback: impl FnOnce() -> F,
        operation: impl FnOnce(Type<'db>) -> F,
    ) -> F {
        self.map_if_unfolded(db, env, operation)
            .unwrap_or_else(fallback)
    }

    pub(crate) fn map_or<F>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        fallback: F,
        operation: impl FnOnce(Type<'db>) -> F,
    ) -> F {
        self.map_if_unfolded(db, env, operation).unwrap_or(fallback)
    }

    fn map_if_unfolded<F>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        operation: impl FnOnce(Type<'db>) -> F,
    ) -> Option<F> {
        let unfolded = self.unfold(db, env);
        if unfolded == Type::Recursive(self) {
            None
        } else {
            Some(operation(unfolded))
        }
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// A sufficient relation between materializations of the same recursive constructor.
    /// Restricting the family of argument materializations restricts the family of
    /// instantiated bodies, even when unfolding would grow the arguments indefinitely.
    /// Structural comparison remains an alternative: aliases need not be injective.
    pub(super) fn when_recursive_arguments_relate(
        &self,
        db: &'db dyn Db,
        source: RecursiveType<'db>,
        target: RecursiveType<'db>,
    ) -> ConstraintSet<'db, 'c> {
        if !matches!(
            self.relation,
            TypeRelation::Subtyping | TypeRelation::Assignability
        ) || source.constructor(db) != target.constructor(db)
        {
            return self.never();
        }
        let (Some(source_arguments), Some(target_arguments)) =
            (source.arguments(db), target.arguments(db))
        else {
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
        // Top-to-bottom requires the body itself to be fully static. Equal static
        // arguments do not rule out a fixed `Any` in that body.
        if matches!(
            (source_kind, target_kind),
            (MaterializationKind::Top, MaterializationKind::Bottom)
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

/// Materialize an unfolding lazily, keeping the marked binder as the recursive fallback.
///
/// Comparing a recursive specialization with its materialization can request this same unfolding
/// before it has finished materializing. Returning the marked binder closes that cycle while
/// preserving the requested materialization polarity.
#[salsa::tracked(
    returns(copy),
    cycle_initial=|_, _, recursive: RecursiveType<'db>| Type::Recursive(recursive),
    heap_size=ruff_memory_usage::heap_size
)]
fn materialized_unfold<'db>(db: &'db dyn Db, recursive: RecursiveType<'db>) -> Type<'db> {
    let Some(kind) = recursive.materialization_kind(db) else {
        debug_assert!(
            false,
            "materialized unfolding requires a materialization kind"
        );
        return Type::Recursive(recursive);
    };
    let env = recursive.environment(db);
    let unfolded = recursive.with_materialization(db, None).unfold(db, &env);
    unfolded.apply_type_mapping(
        db,
        &env,
        &TypeMapping::Materialize(kind),
        TypeContext::default(),
    )
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

/// Collect references in one unfolding without descending into their definitions.
/// Named aliases expose only their displayed arguments.
struct RecursiveDisplayReferences<'env, 'db> {
    env: &'env ProgramEnvironment<'db>,
    references: RefCell<Vec<RecursiveType<'db>>>,
}

impl<'db> TypeVisitor<'db> for RecursiveDisplayReferences<'_, 'db> {
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if let TypeKind::NonAtomic(node) = TypeKind::from(ty) {
            walk_non_atomic_type(db, node, self);
        }
    }

    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if recursive.alias(db).is_none() {
            self.references.borrow_mut().push(recursive);
        } else if let Some(arguments) = recursive.arguments(db) {
            walk_specialization_types(db, arguments, self);
        }
    }

    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        if let Some(arguments) = alias.specialization(db) {
            walk_specialization_types(db, arguments, self);
        }
    }
}

/// A syntactic walk that counts binders without unfolding or normalizing types.
struct RecursiveReferences<'env, 'db> {
    env: &'env ProgramEnvironment<'db>,
    /// Number of surrounding recursive binders entered from the inspected root.
    /// A variable is bound within that root exactly when its de Bruijn index is
    /// smaller than this count.
    depth: Cell<u32>,
    found: Cell<bool>,
    query: &'env dyn Fn(RecursiveVar<'db>) -> bool,
    /// The same interned subtree can be bound at one depth and escaping at another.
    seen: RefCell<FxHashSet<(Type<'db>, u32)>>,
}

impl<'env, 'db> RecursiveReferences<'env, 'db> {
    /// Inspect a type without assuming any binders outside it. A raw body can have
    /// escaping references even when its enclosing `RecursiveType` is closed.
    fn contains_escaping(
        db: &'db dyn Db,
        env: &'env ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> bool {
        let visitor = Self {
            env,
            depth: Cell::new(0),
            found: Cell::new(false),
            query: &|_| true,
            seen: RefCell::default(),
        };
        visitor.visit_type(db, ty);
        visitor.found.get()
    }

    /// Equation indices referenced from a raw body, excluding nested local binders.
    fn indices(db: &'db dyn Db, env: &'env ProgramEnvironment<'db>, ty: Type<'db>) -> Vec<usize> {
        let indices = RefCell::new(Vec::new());
        let query = |reference: RecursiveVar<'db>| {
            indices.borrow_mut().push(reference.index(db));
            false
        };
        let visitor = RecursiveReferences {
            env,
            depth: Cell::new(0),
            found: Cell::new(false),
            query: &query,
            seen: RefCell::default(),
        };
        visitor.visit_type(db, ty);
        indices.into_inner()
    }
}

impl<'db> TypeVisitor<'db> for RecursiveReferences<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }
    /// At depth `d`, only indices below `d` have a binder within the inspected root.
    /// Revisit shared subtrees when the depth changes, since their binding can change.
    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if self.found.get() {
            return;
        }
        if let Type::RecursiveVar(reference) = ty {
            if reference.depth(db) >= self.depth.get() {
                self.found.set((self.query)(reference));
            }
            if let Some(arguments) = reference.arguments(db) {
                walk_specialization_types(db, arguments, self);
            }
        } else if self.seen.borrow_mut().insert((ty, self.depth.get()))
            && let TypeKind::NonAtomic(node) = TypeKind::from(ty)
        {
            walk_non_atomic_type(db, node, self);
        }
    }
    /// Count this binder only inside its body. Application arguments remain in the
    /// surrounding scope, and the original depth is restored before visiting siblings.
    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if let Some(arguments) = recursive.arguments(db) {
            walk_specialization_types(db, arguments, self);
        }
        let depth = self.depth.get();
        self.depth.set(depth + 1);
        for body in recursive.graph(db).bodies(db) {
            self.visit_type(db, *body);
        }
        self.depth.set(depth);
    }
    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        if let Some(specialization) = alias.specialization(db) {
            walk_specialization_types(db, specialization, self);
        }
    }
}

impl<'db> Type<'db> {
    /// Reject a bare recursive variable at a semantic-operation boundary.
    /// Binding and unfolding check nested bodies; ordinary type operations must
    /// not rescan the entire type graph just to validate each operand.
    pub(super) const fn assert_not_recursive_var(self) {
        debug_assert!(
            !matches!(self, Self::RecursiveVar(_)),
            "semantic operation on an unbound recursive variable"
        );
    }

    fn assert_no_unbound_recursive_vars(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) {
        debug_assert!(
            !RecursiveReferences::contains_escaping(db, env, self),
            "semantic operation on an unbound recursive variable"
        );
    }
}
