//! Binding and capture-avoiding substitution for structural recursive types.
//!
//! `RecursiveVar` is syntax, with no standalone type semantics. Only structural
//! substitutions may inspect an open body. Ordinary type operations receive its
//! closed unfolding, including during intermediate normalization steps.

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashSet;
use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::generics::{ApplySpecialization, Specialization, walk_specialization_types};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::variance::{VarianceInferable, VarianceOrigin};
use super::visitor::{TypeKind, TypeVisitor, walk_non_atomic_type};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, GenericContext, MaterializationKind, Type,
    TypeAliasType, TypeContext, TypeMapping, VarianceTerm,
};
use crate::{Db, ProgramEnvironment};

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
    #[returns(copy)]
    arguments: Option<Specialization<'db>>,
}

impl get_size2::GetSize for RecursiveVar<'_> {}

impl<'db> RecursiveVar<'db> {
    /// Whether this reference belongs to the innermost recursive binder.
    pub(super) fn is_innermost(self, db: &'db dyn Db) -> bool {
        self.depth(db) == 0
    }

    /// Unfold references whose index equals the number of nested binders entered
    /// by the visitor. Smaller indices belong to inner binders and stay unchanged.
    /// Larger indices escape the closed input; binding also rejects equal indices,
    /// since its input cannot already refer to the binder being introduced.
    pub(super) fn apply_type_mapping(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let arguments = self
            .arguments(db)
            .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
        match mapping {
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Unfold(recursive)))
                if self.depth(db) == visitor.recursive_depth =>
            {
                Type::Recursive(recursive.with_arguments(db, arguments))
            }
            TypeMapping::Recursive(_) if self.depth(db) < visitor.recursive_depth => {
                Type::RecursiveVar(Self::new_internal(db, self.depth(db), arguments))
            }
            _ => unreachable!("semantic operation on an unbound recursive variable"),
        }
    }
}

/// A structural substitution that only the recursive-type binder can construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub struct RecursiveMapping<'db>(RecursiveSubstitution<'db>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum RecursiveSubstitution<'db> {
    Unfold(RecursiveType<'db>),
    Bind(RecursiveType<'db>),
}

/// The query cycle that introduced a provisional recursive type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecursiveCycle(salsa::Id);

impl get_size2::GetSize for RecursiveCycle {}

/// A recursive type whose raw body is private. Unfolding substitutes closed types
/// for references before exposing the body to ordinary type operations.
/// Use the binding operations in this module to construct recursive types.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// The defining symbol of the implicit alias, including for qualified references.
    #[returns(copy)]
    pub(super) definition: Definition<'db>,
    /// Distinguishes the provisional types of different alias queries.
    #[returns(copy)]
    cycle: RecursiveCycle,
    #[returns(copy)]
    body: Type<'db>,
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
            definition,
            RecursiveCycle(cycle),
            Type::RecursiveVar(RecursiveVar::new_internal(db, 0, arguments)),
            arguments,
            None,
        ))
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
    fn bind(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original: Type<'db>,
    ) -> Type<'db> {
        original.assert_no_unbound_recursive_vars(db, env);
        let body = original.apply_type_mapping_impl(
            db,
            &TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Bind(self))),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        // Binding changes a closed type only by introducing references to this binder.
        debug_assert_eq!(
            body != original,
            RecursiveReferences::contains_escaping(db, env, body)
        );

        // Alias arguments can expose a reference without introducing a container.
        let result = if body.has_unguarded_alias_cycle(db) {
            Type::divergent(self.cycle(db).0)
        } else if body == original {
            body
        } else {
            Type::Recursive(Self::new_internal(
                db,
                self.definition(db),
                self.cycle(db),
                body,
                self.arguments(db),
                None,
            ))
        };
        result.assert_no_unbound_recursive_vars(db, env);
        result
    }

    fn with_arguments(self, db: &'db dyn Db, arguments: Option<Specialization<'db>>) -> Self {
        Self::new_internal(
            db,
            self.definition(db),
            self.cycle(db),
            self.body(db),
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
            self.definition(db),
            self.cycle(db),
            self.body(db),
            self.arguments(db),
            materialization,
        )
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
        ProgramEnvironment::from_definition(self.definition(db))
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

    /// Structural binding replaces matching constructors with a reference whose index
    /// equals the visitor's depth. Other recursive bodies add one binder to that depth;
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
                if self.cycle(db) == target.cycle(db)
                    && self.body(db) == target.body(db)
                    && self.materialization_kind(db) == target.materialization_kind(db) =>
            {
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    visitor.recursive_depth,
                    arguments,
                ))
            }
            TypeMapping::Recursive(_) => {
                let nested = visitor.with_recursive_binder();
                let body = self
                    .body(db)
                    .apply_type_mapping_impl(db, mapping, tcx, &nested);
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::Recursive(Self::new_internal(
                    db,
                    self.definition(db),
                    self.cycle(db),
                    body,
                    arguments,
                    self.materialization_kind(db),
                ))
            }
            TypeMapping::ApplySpecialization(_)
            | TypeMapping::ApplySpecializationWithMaterialization { .. }
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
            TypeMapping::Materialize(kind) => {
                Type::Recursive(if self.materialization_kind(db).is_some() {
                    self
                } else {
                    self.with_materialization(db, Some(*kind))
                })
            }
            _ => {
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

/// A syntactic walk that counts binders without unfolding or normalizing types.
struct RecursiveReferences<'env, 'db> {
    env: &'env ProgramEnvironment<'db>,
    /// Number of surrounding recursive binders entered from the inspected root.
    /// A variable is bound within that root exactly when its de Bruijn index is
    /// smaller than this count.
    depth: Cell<u32>,
    found: Cell<bool>,
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
            seen: RefCell::default(),
        };
        visitor.visit_type(db, ty);
        visitor.found.get()
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
                self.found.set(true);
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
        self.visit_type(db, recursive.body(db));
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
