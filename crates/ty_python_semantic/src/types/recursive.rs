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
//! the other direction: it replaces applications of a recursive type with variables
//! and encloses the resulting open body in a `RecursiveType`. Both operations respect
//! nested binders. A variable refers to the nearest enclosing binder with the same
//! `RecursiveCycle`; substitution does not enter that binder's body when it binds the
//! target variable. The binder's arguments are outside its scope and are still substituted.
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
//! `RecursiveCycle` and unspecialized arguments `[list[T]]`.
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

use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::generics::{ApplySpecialization, Specialization};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::type_alias::AliasCycleSummary;
use super::variance::{VarianceInferable, VarianceOrigin};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, GenericContext, MaterializationKind, Type,
    TypeContext, TypeMapping, VarianceTerm,
};
use crate::{Db, ProgramEnvironment};

/// A recursive variable named by its binder's query cycle.
/// An escaping reference has no type semantics; in particular, it is neither a
/// gradual type nor an assignability operand.
/// Only recursive-type binding and substitution operations may construct or operate on recursive variables.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveVar<'db> {
    /// Refers to the nearest enclosing recursive binder with this cycle identity.
    #[returns(copy)]
    cycle: RecursiveCycle,
    /// The unspecialized arguments of this occurrence, in the alias definition's scope.
    /// For `Tree = tuple[T, "Tree[list[T]] | None"]`, these are `[list[T]]`.
    /// Unfolding substitutes the enclosing application's arguments for the type parameters.
    #[returns(copy)]
    arguments: Option<Specialization<'db>>,
}

impl get_size2::GetSize for RecursiveVar<'_> {}

impl<'db> RecursiveVar<'db> {
    /// Unfold references to the target cycle, retaining variables bound by other cycles.
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
            )) if self.cycle(db) == recursive.cycle(db) => {
                Type::Recursive(recursive.with_arguments(db, arguments))
            }
            TypeMapping::ApplyRecursiveSubstitution(_) => {
                Type::RecursiveVar(Self::new_internal(db, self.cycle(db), arguments))
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
    /// Replace references to a binder with applications of its recursive constructor.
    /// In the module example, this replaces `F[list[T]]` with `Tree[list[T]]`.
    Unfold(RecursiveType<'db>),
    /// Replace applications of the target binder with variables to form an open body.
    /// In the module example, this replaces `Tree[list[T]]` with `F[list[T]]`.
    Bind(RecursiveCycle),
}

impl RecursiveSubstitution<'_> {
    fn cycle(self, db: &dyn Db) -> RecursiveCycle {
        match self {
            Self::Unfold(recursive) => recursive.cycle(db),
            Self::Bind(cycle) => cycle,
        }
    }
}

/// Identifies an alias query and names its recursive binder and variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecursiveCycle(salsa::Id);

impl get_size2::GetSize for RecursiveCycle {}

/// An application of a structural recursive type constructor.
/// The private body remains unspecialized; `arguments` records this application's
/// substitution for the constructor's type parameters. Unfolding replaces recursive
/// references with closed types, then applies that substitution before exposing the
/// result to ordinary type operations.
/// Use the binding operations in this module to construct recursive types.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// The defining symbol of the implicit alias, including for qualified references.
    #[returns(copy)]
    pub(super) definition: Definition<'db>,
    /// Names the binder and distinguishes provisional types of different alias queries.
    #[returns(copy)]
    cycle: RecursiveCycle,
    #[returns(copy)]
    body: Type<'db>,
    /// The actual arguments for this application, which may themselves contain type variables.
    /// They are applied when unfolding; the stored body remains unspecialized.
    #[returns(copy)]
    pub(super) arguments: Option<Specialization<'db>>,
    /// The lazy materialization applied to this recursive alias, if any.
    #[returns(copy)]
    pub(super) materialization_kind: Option<MaterializationKind>,
}

impl get_size2::GetSize for RecursiveType<'_> {}

#[salsa::tracked]
impl<'db> RecursiveType<'db> {
    /// Summarize the open constructor body without applying semantic substitutions.
    pub(super) fn cycle_summary(self, db: &'db dyn Db) -> &'db AliasCycleSummary<'db> {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|db, id, _, ()| AliasCycleSummary::from_type(db, Type::divergent_alias(id)),
            heap_size=ruff_memory_usage::heap_size
        )]
        fn cycle_summary_impl<'db>(
            db: &'db dyn Db,
            recursive: RecursiveType<'db>,
            (): (),
        ) -> AliasCycleSummary<'db> {
            let mut summary = AliasCycleSummary::from_type(db, recursive.body(db));
            // Nested bodies can refer to an enclosing binder. Close only the cycle marker,
            // so recovery never exposes an unbound variable as a standalone type.
            if let Some(Type::RecursiveVar(variable)) = summary.cycle {
                summary.cycle = Some(Type::divergent_alias(variable.cycle(db).0));
            }
            summary
        }

        cycle_summary_impl(db, self.constructor(db), ())
    }

    /// Seed a query cycle with `μa. a`, using the same identity for binder and variable.
    pub(super) fn initial(
        db: &'db dyn Db,
        definition: Definition<'db>,
        cycle: salsa::Id,
        parameters: Option<GenericContext<'db>>,
    ) -> Self {
        let cycle = RecursiveCycle(cycle);
        let arguments = parameters.map(|parameters| parameters.identity_specialization(db));
        Self::new_internal(
            db,
            definition,
            cycle,
            Type::RecursiveVar(RecursiveVar::new_internal(db, cycle, arguments)),
            arguments,
            None,
        )
    }

    /// Close recursive occurrences after inferring an alias's constructor expression.
    pub(super) fn recover(
        db: &'db dyn Db,
        definition: Definition<'db>,
        cycle: salsa::Id,
        parameters: Option<GenericContext<'db>>,
        result: Type<'db>,
    ) -> Type<'db> {
        // Shared dependencies can still contain older iterations of this alias. They refer
        // to the same definition even when their provisional bodies differ. Derive the binder
        // from the query inputs: an earlier result might not expose recursive references yet.
        Self::initial(db, definition, cycle, parameters).bind(
            db,
            &ProgramEnvironment::from_definition(definition),
            result,
        )
    }

    /// Bind references to this alias's query cycle in a closed inference result.
    /// Each bound occurrence retains its arguments and refers to this constructor's cycle.
    fn bind(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original: Type<'db>,
    ) -> Type<'db> {
        let body = original.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Bind(self.cycle(db)),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        // Alias arguments can expose a reference without introducing a container.
        if body.has_unguarded_alias_cycle(db) {
            Type::divergent_alias(self.cycle(db).0)
        } else if body == original {
            // Binding changes a closed type only by introducing references to this binder.
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
        }
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

    /// The source alias's definition and name, if this binder comes from an alias.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "Keep alias metadata optional for inferred recursive types"
    )]
    pub(super) fn alias(self, db: &'db dyn Db) -> Option<(Definition<'db>, &'db str)> {
        // Only implicit alias inference constructs recursive types at present.
        Some((self.definition(db), self.name(db)))
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
    ///
    /// Report whether unfolding returns exactly `Type::Recursive(self)`. An unfolded
    /// type can still contain recursive references, so callers must retain their recursion guards.
    pub fn unfold(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> UnfoldResult<'db> {
        // A growing specialization cannot converge by repeating the same query key. Materialize
        // its closed unfolding directly, under the caller's recursion guard, instead.
        let unfolded = if self.materialization_kind(db).is_some()
            && !self.may_have_unbounded_specialization(db)
        {
            materialized_unfold(db, self)
        } else {
            let unfolded = self.unfolded_body(db);
            match self.materialization_kind(db) {
                Some(kind) => unfolded.apply_type_mapping(
                    db,
                    env,
                    &TypeMapping::Materialize(kind),
                    TypeContext::default(),
                ),
                None => unfolded,
            }
        };
        if unfolded == Type::Recursive(self) {
            UnfoldResult::Unchanged(self)
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
        let unfolded = self.body(db).apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Unfold(self),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
        );
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

    /// Substitute by cycle identity, respecting the scope of nested recursive binders.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        match mapping {
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Bind(cycle),
            )) if self.cycle(db) == *cycle && self.materialization_kind(db).is_none() => {
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::RecursiveVar(RecursiveVar::new_internal(db, self.cycle(db), arguments))
            }
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(substitution)) => {
                // This binder shadows the target in its body, but not in its arguments.
                let body = if self.cycle(db) == substitution.cycle(db) {
                    self.body(db)
                } else {
                    self.body(db)
                        .apply_type_mapping_impl(db, mapping, tcx, visitor)
                };
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
            TypeMapping::Materialize(_) if self.materialization_kind(db).is_some() => {
                Type::Recursive(self)
            }
            TypeMapping::Materialize(kind) => {
                visitor.visit(db, Type::Recursive(self), mapping, || {
                    self.unfold(db, visitor.env)
                        .map(|unfolded| {
                            let mapped =
                                unfolded.apply_type_mapping_impl(db, mapping, tcx, visitor);
                            // Preserve static aliases, including recursive references that the
                            // visitor leaves unchanged while materializing their enclosing body.
                            Type::Recursive(if mapped == unfolded {
                                self
                            } else {
                                self.with_materialization(db, Some(*kind))
                            })
                        })
                        .into_type()
                })
            }
            TypeMapping::EagerExpansion => {
                visitor.visit(db, Type::Recursive(self), mapping, || {
                    // Expand arguments only where the body exposes them. Expanding stored arguments
                    // first can feed a recursive alias's previous approximation into its own arguments.
                    self.unfold(db, visitor.env)
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
            _ => visitor.visit(db, Type::Recursive(self), mapping, || {
                // Map arguments before unfolding so recursive backedges retain their mapped
                // arguments. Keep the application's materialization throughout the traversal.
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                let recursive = self.with_arguments(db, arguments);
                recursive
                    .unfold(db, visitor.env)
                    .map(|unfolded| {
                        let mapped = unfolded.apply_type_mapping_impl(db, mapping, tcx, visitor);
                        if mapped == unfolded {
                            Type::Recursive(recursive)
                        } else {
                            mapped
                        }
                    })
                    .into_type()
            }),
        }
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
        // Top-to-bottom also requires a static body: equal arguments do not rule
        // out a fixed `Any`. Unchanged materializations retain the original binder.
        let unmaterialized_target = Type::Recursive(target.with_materialization(db, None));
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
    let unfolded = recursive
        .with_materialization(db, None)
        .unfold(db, &env)
        .into_type();
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

impl Type<'_> {
    /// Reject a bare recursive variable at a semantic-operation boundary.
    pub(super) const fn assert_not_recursive_var(self) {
        debug_assert!(
            !matches!(self, Self::RecursiveVar(_)),
            "semantic operation on an unbound recursive variable"
        );
    }
}
