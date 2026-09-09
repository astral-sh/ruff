//! Binding and capture-avoiding substitution for structural recursive types.
//!
//! `RecursiveVar` is syntax, with no standalone type semantics. Only structural
//! substitutions may inspect an open body. Ordinary type operations receive its
//! closed unfolding, including during intermediate normalization steps.

use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::generics::{ApplySpecialization, Specialization};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::variance::{VarianceInferable, VarianceOrigin};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, GenericContext, MaterializationKind, Type,
    TypeContext, TypeMapping, VarianceTerm,
};
use crate::{Db, ProgramEnvironment};

/// A recursive variable named by its binder's query cycle.
/// An escaping reference has no type semantics; in particular, it is neither a
/// gradual type nor an assignability operand.
/// Only binding and substitution operations may construct recursive variables.
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
                if self.cycle(db) == recursive.cycle(db) =>
            {
                Type::Recursive(recursive.with_arguments(db, arguments))
            }
            TypeMapping::Recursive(_) => {
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
    Unfold(RecursiveType<'db>),
    Bind(RecursiveBinding<'db>),
}

impl RecursiveSubstitution<'_> {
    fn cycle(self, db: &dyn Db) -> RecursiveCycle {
        match self {
            Self::Unfold(recursive) | Self::Bind(RecursiveBinding::Constructor(recursive)) => {
                recursive.cycle(db)
            }
            Self::Bind(RecursiveBinding::Alias(cycle)) => cycle,
        }
    }
}

/// Inference binds references to an alias across iterations; transformations bind an exact type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum RecursiveBinding<'db> {
    Alias(RecursiveCycle),
    Constructor(RecursiveType<'db>),
}

impl<'db> RecursiveBinding<'db> {
    fn matches(self, db: &'db dyn Db, recursive: RecursiveType<'db>) -> bool {
        match self {
            Self::Alias(cycle) => {
                recursive.cycle(db) == cycle && recursive.materialization_kind(db).is_none()
            }
            Self::Constructor(target) => {
                recursive.cycle(db) == target.cycle(db)
                    && recursive.body(db) == target.body(db)
                    && recursive.materialization_kind(db) == target.materialization_kind(db)
            }
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
///
/// For example, with type variable `T`, the alias
/// `Tree = tuple[T, "Tree[list[T]] | None"]` has the recursive constructor:
///
/// ```text
/// μF. λT. tuple[T, F[list[T]] | None]
/// ```
///
/// Here `μF` binds the recursive constructor, and `λT` binds its type parameter.
/// The occurrence `F[list[T]]` is stored as `RecursiveVar` with the constructor's
/// `RecursiveCycle` and unspecialized arguments `[list[T]]`.
///
/// To infer the container subscript `x[1]` for `x: Tree[int]`, first unfold `x`'s type.
/// Unfolding replaces references to the recursive binder with the recursive type
/// itself. Writing `B[a := R]` for capture-avoiding substitution of `R` for `a` in `B`:
///
/// ```text
/// unfold(μa. B) = B[a := μa. B]
/// ```
///
/// For `Tree[int]`, substitute the constructor for `F`, then apply `T := int`:
///
/// ```text
/// unfold((μF. λT. tuple[T, F[list[T]] | None])[int])
/// = tuple[int, (μF. λT. tuple[T, F[list[T]] | None])[list[int]] | None]
/// = tuple[int, Tree[list[int]] | None]
/// ```
///
/// Tuple subscripting then selects the element at index 1: `x[1]: Tree[list[int]] | None`.
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

impl<'db> RecursiveType<'db> {
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
            RecursiveBinding::Alias(RecursiveCycle(cycle)),
        )
    }

    /// Bind occurrences of this recursive constructor in a closed result.
    /// Each bound occurrence retains its arguments and refers to this constructor's cycle.
    fn bind(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original: Type<'db>,
        binding: RecursiveBinding<'db>,
    ) -> Type<'db> {
        let body = original.apply_type_mapping_impl(
            db,
            &TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Bind(binding))),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        // Alias arguments can expose a reference without introducing a container.
        if body.has_unguarded_alias_cycle(db) {
            Type::divergent(self.cycle(db).0)
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
    pub fn unfold(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
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

    /// Substitute by cycle identity, respecting the scope of nested recursive binders.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        match mapping {
            TypeMapping::Recursive(RecursiveMapping(RecursiveSubstitution::Bind(binding)))
                if binding.matches(db, self) =>
            {
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::RecursiveVar(RecursiveVar::new_internal(db, self.cycle(db), arguments))
            }
            TypeMapping::Recursive(RecursiveMapping(substitution)) => {
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
                        constructor.bind(
                            db,
                            visitor.env,
                            mapped,
                            RecursiveBinding::Constructor(constructor),
                        )
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

impl Type<'_> {
    /// Reject a bare recursive variable at a semantic-operation boundary.
    pub(super) const fn assert_not_recursive_var(self) {
        debug_assert!(
            !matches!(self, Self::RecursiveVar(_)),
            "semantic operation on an unbound recursive variable"
        );
    }
}
