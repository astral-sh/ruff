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

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};
use salsa::plumbing::AsId;
use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::cyclic::TypeIdentity;
use super::generics::{ApplySpecialization, Specialization, walk_specialization_types};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::type_alias::{AliasCycleSummary, TypeAliasType};
use super::variance::{VarianceInferable, VarianceOrigin};
use super::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext,
    MaterializationKind, Type, TypeContext, TypeMapping, VarianceTerm,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

mod parameters;
use parameters::RecursiveParameters;

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
    /// Restore references in subtrees that the semantic mapping did not visit.
    Restore {
        placeholder: RecursiveMappingReference<'db>,
        source: Type<'db>,
    },
    /// Close a body with a fresh opaque reference before applying a semantic mapping.
    Close {
        cycle: RecursiveCycle,
        placeholder: RecursiveMappingReference<'db>,
    },
    /// Replace only this transformation's fresh references with bound variables.
    BindFresh(RecursiveMappingReference<'db>),
    /// Abstract a stored constructor while reducing the parameters of nested binders.
    Abstract {
        constructor: RecursiveType<'db>,
        placeholder: RecursiveMappingReference<'db>,
    },
}

impl RecursiveSubstitution<'_> {
    fn cycle(self, db: &dyn Db) -> RecursiveCycle {
        match self {
            Self::Unfold(recursive) => recursive.cycle(db),
            Self::Bind(cycle) => cycle,
            Self::Restore { placeholder, .. } => placeholder.cycle(db),
            Self::Close { cycle, .. } => cycle,
            Self::BindFresh(placeholder) => placeholder.cycle(db),
            Self::Abstract { constructor, .. } => constructor.cycle(db),
        }
    }
}

/// Names a recursive binder.
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
    /// Whether the stored body still denotes the source alias or a transformation of it.
    #[returns(copy)]
    origin: RecursiveOrigin<'db>,
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

/// Retains the source environment without naming a transformed body after the original alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum RecursiveOrigin<'db> {
    Alias(Definition<'db>),
    Transformed(Definition<'db>),
}

#[salsa::tracked]
impl<'db> RecursiveType<'db> {
    pub(super) fn definition(self, db: &'db dyn Db) -> Definition<'db> {
        match self.origin(db) {
            RecursiveOrigin::Alias(definition) | RecursiveOrigin::Transformed(definition) => {
                definition
            }
        }
    }

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
            RecursiveOrigin::Alias(definition),
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
                self.origin(db),
                self.cycle(db),
                body,
                self.arguments(db),
                None,
            ))
        }
    }

    /// Apply this constructor to new arguments, preserving its body and materialization.
    pub(super) fn with_arguments(
        self,
        db: &'db dyn Db,
        arguments: Option<Specialization<'db>>,
    ) -> Self {
        Self::new_internal(
            db,
            self.origin(db),
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
            self.origin(db),
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

    /// The alias this body denotes, if it has not been structurally transformed.
    pub(super) fn alias(self, db: &'db dyn Db) -> Option<(Definition<'db>, &'db str)> {
        match self.origin(db) {
            RecursiveOrigin::Alias(definition) => Some((definition, self.name(db))),
            RecursiveOrigin::Transformed(_) => None,
        }
    }

    /// Restore the formal arguments for constructor analysis.
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
        if self.is_cycle_seed(db) && !mapping.is_structural() {
            if let TypeMapping::Materialize(kind) = mapping {
                return Type::Recursive(self.with_materialization(db, Some(*kind)));
            }
            let arguments = self
                .arguments(db)
                .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
            return Type::Recursive(self.with_arguments(db, arguments));
        }
        let substitutes_arguments = mapping.substitutes_variables()
            && (matches!(mapping, TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(arguments))
                    if self.parameters(db) == Some(arguments.generic_context(db)))
                || !self.maps_free_variables(db, mapping, visitor, &FxHashSet::default()));
        if !mapping.is_structural()
            && !matches!(mapping, TypeMapping::EagerExpansion)
            && !substitutes_arguments
        {
            return RecursiveTypeMapping::apply(db, Type::Recursive(self), mapping, tcx, visitor);
        }
        if substitutes_arguments {
            let arguments = self
                .arguments(db)
                .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
            return Type::Recursive(self.with_arguments(db, arguments));
        }
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
                    self.origin(db),
                    self.cycle(db),
                    body,
                    arguments,
                    self.materialization_kind(db),
                ))
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
            _ => RecursiveTypeMapping::apply(db, Type::Recursive(self), mapping, tcx, visitor),
        }
    }

    fn is_cycle_seed(self, db: &'db dyn Db) -> bool {
        matches!(self.body(db), Type::RecursiveVar(variable) if variable.cycle(db) == self.cycle(db))
    }

    /// Parameters are substituted through arguments, but captured variables belong to the body.
    fn maps_free_variables(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        bound: &FxHashSet<BoundTypeVarIdentity<'db>>,
    ) -> bool {
        let mut bound = bound.clone();
        if let Some(parameters) = self.parameters(db) {
            bound.extend(
                parameters
                    .variables(db)
                    .map(|variable| variable.identity(db)),
            );
        }
        let search = FreeVariableMapping {
            mapping,
            visitor,
            bound: &bound,
            changed: Cell::new(false),
            seen: TypeCollector::default(),
        };
        search.visit_type(db, self.body(db));
        search.changed.get()
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

/// Inspect an open body without unfolding references or substituting its bound parameters.
struct FreeVariableMapping<'a, 'db> {
    mapping: &'a TypeMapping<'a, 'db>,
    visitor: &'a ApplyTypeMappingVisitor<'a, 'db>,
    bound: &'a FxHashSet<BoundTypeVarIdentity<'db>>,
    changed: Cell<bool>,
    seen: TypeCollector<'db>,
}

impl<'db> TypeVisitor<'db> for FreeVariableMapping<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.visitor.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        if self.changed.get() {
            return;
        }
        let arguments = match ty {
            Type::TypeVar(variable) => {
                // P.args and P.kwargs belong to the binder that owns P.
                let mut identity = variable.identity(db);
                identity.paramspec_attr = None;
                if !self.bound.contains(&identity)
                    && ty.apply_type_mapping_impl(
                        db,
                        self.mapping,
                        TypeContext::default(),
                        self.visitor,
                    ) != ty
                {
                    self.changed.set(true);
                }
                return;
            }
            Type::Recursive(recursive) => {
                if recursive.maps_free_variables(db, self.mapping, self.visitor, self.bound) {
                    self.changed.set(true);
                    return;
                }
                recursive.arguments(db)
            }
            Type::RecursiveVar(variable) => variable.arguments(db),
            Type::TypeAlias(alias) => alias.specialization(db),
            _ => {
                walk_type_with_recursion_guard(db, ty, self, &self.seen);
                return;
            }
        };
        if let Some(arguments) = arguments {
            walk_specialization_types(db, arguments, self);
        }
    }
}

/// Metadata for a temporary reference. Its fresh type is an opaque substitution variable;
/// constructor arguments remain available to the mapping without giving that variable a body.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct RecursiveMappingReference<'db> {
    scope: Type<'db>,
    index: usize,
    #[returns(copy)]
    arguments: Option<Specialization<'db>>,
    #[returns(copy)]
    materialization_kind: Option<MaterializationKind>,
}

impl get_size2::GetSize for RecursiveMappingReference<'_> {}

impl<'db> RecursiveMappingReference<'db> {
    fn cycle(self, db: &'db dyn Db) -> RecursiveCycle {
        RecursiveCycle(Self::new(db, self.scope(db), self.index(db), None, None).as_id())
    }

    fn with_arguments(self, db: &'db dyn Db, arguments: Option<Specialization<'db>>) -> Self {
        Self::new(
            db,
            self.scope(db),
            self.index(db),
            arguments,
            self.materialization_kind(db),
        )
    }

    /// Rebind arguments by position when closing a body under renamed formal parameters.
    fn rebind_arguments(self, db: &'db dyn Db, arguments: Option<Specialization<'db>>) -> Self {
        let arguments = self
            .arguments(db)
            .zip(arguments)
            .map(|(parameters, arguments)| {
                Specialization::new(
                    db,
                    parameters.generic_context(db),
                    arguments.types(db),
                    arguments.materialization_kind(db),
                    None,
                )
            });
        self.with_arguments(db, arguments)
    }

    /// Retain the surviving parameters when binding or restoring a reduced constructor.
    fn project_arguments(
        self,
        db: &'db dyn Db,
        arguments: Option<Specialization<'db>>,
    ) -> Option<Specialization<'db>> {
        let parameters = self.arguments(db)?.generic_context(db);
        let arguments = arguments?;
        let types = arguments
            .generic_context(db)
            .variables(db)
            .zip(arguments.types(db))
            .filter_map(|(variable, ty)| {
                parameters
                    .contains(db, variable.identity(db))
                    .then_some(*ty)
            })
            .collect::<Box<[_]>>();
        Some(Specialization::new(
            db,
            parameters,
            types,
            arguments.materialization_kind(db),
            None,
        ))
    }

    fn restore(self, db: &'db dyn Db, source: Type<'db>) -> Type<'db> {
        if self.arguments(db).is_none() && self.materialization_kind(db).is_none() {
            return source;
        }
        match source {
            Type::Recursive(recursive) => Type::Recursive(
                recursive
                    .with_arguments(db, self.arguments(db))
                    .with_materialization(db, self.materialization_kind(db)),
            ),
            Type::TypeAlias(alias) => {
                let alias = match self.arguments(db) {
                    Some(arguments) => alias.apply_specialization(db, |_| arguments),
                    None => alias,
                };
                Type::TypeAlias(alias.with_materialization_kind(db, self.materialization_kind(db)))
            }
            source => source,
        }
    }
}

/// Keeps the active recursive references closed while a mapping transforms their bodies.
/// Each completed body binds its own placeholder; the result contains no pending mapping.
/// Growing constructors use separate formal arguments for each mapping state, so occurrences
/// of a parameter in opposite variances can have different results without unfolding forever.
///
/// The probe visits each alias in each mapping state and lexical scope once. The transformation
/// shares completed results in the same parameter environment, but never results containing
/// placeholders for enclosing frames. Substitutions need separate visitor caches; materialization
/// equivalence checks share their cache across those visitors to avoid repeating nested mappings.
pub(super) struct RecursiveTypeMapping<'a, 'db> {
    scope: Type<'db>,
    mappings: Vec<TypeMapping<'a, 'db>>,
    active: RefCell<Vec<RecursiveMappingFrame<'db>>>,
    next_binder: Cell<usize>,
    references: Rc<RefCell<FxHashMap<Type<'db>, RecursiveMappingReference<'db>>>>,
    completed: RefCell<FxHashMap<RecursiveMappingKey<'db>, Type<'db>>>,
    probe: Option<&'a RecursiveMappingProbe<'db>>,
    outer: Option<&'a RecursiveTypeMapping<'a, 'db>>,
    /// Function variables reaching each formal argument, used to bind returned callables.
    parameter_sources: FxHashMap<BoundTypeVarIdentity<'db>, FxOrderSet<BoundTypeVarInstance<'db>>>,
}

#[derive(PartialEq, Eq, Hash)]
struct RecursiveMappingKey<'db> {
    source: Type<'db>,
    state: usize,
    context: TypeContext<'db>,
    parameters: Box<[Specialization<'db>]>,
}

struct RecursiveMappingAnalysis<'db> {
    changed: bool,
    parameter_sources: FxHashMap<BoundTypeVarIdentity<'db>, FxOrderSet<BoundTypeVarInstance<'db>>>,
}

#[derive(Default)]
struct RecursiveMappingProbe<'db> {
    changed: Cell<bool>,
    scopes: RefCell<Vec<usize>>,
    nodes: RefCell<Vec<RecursiveProbeNode<'db>>>,
    calls: RefCell<Vec<RecursiveProbeCall<'db>>>,
}

struct RecursiveProbeNode<'db> {
    source: Type<'db>,
    state: usize,
    scopes: Box<[usize]>,
    parameters: Option<GenericContext<'db>>,
    used: FxOrderSet<(BoundTypeVarIdentity<'db>, usize)>,
}

#[derive(Clone, PartialEq, Eq)]
struct RecursiveProbeCall<'db> {
    target: usize,
    arguments: Specialization<'db>,
    scopes: Vec<usize>,
}

#[derive(Clone)]
struct RecursiveMappingFrame<'db> {
    source: Type<'db>,
    state: usize,
    placeholder: RecursiveMappingReference<'db>,
    /// Unvisited references retain the source type, for example in an invariant materialization.
    source_reference: RecursiveMappingReference<'db>,
    parameter_mappings: Box<[Specialization<'db>]>,
}

impl<'a, 'db> RecursiveTypeMapping<'a, 'db> {
    /// Separate substitution caches while retaining references and materialization comparisons.
    fn visitor<'v>(
        &'v self,
        visitor: &'v ApplyTypeMappingVisitor<'_, 'db>,
    ) -> ApplyTypeMappingVisitor<'v, 'db> {
        ApplyTypeMappingVisitor {
            recursive_mapping: Some(self),
            recursion_context: visitor.recursion_context,
            materialize_typevar_bounds_and_defaults: visitor
                .materialize_typevar_bounds_and_defaults,
            ..visitor.for_new_materialization_root()
        }
    }

    fn reference_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        reference: RecursiveMappingReference<'db>,
    ) -> Type<'db> {
        let ty = Type::fresh(db, env, reference.as_id());
        self.references.borrow_mut().insert(ty, reference);
        ty
    }

    fn map_reference(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        reference: RecursiveMappingReference<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if self.mappings.iter().any(|candidate| candidate == mapping) {
            let frame = self
                .active
                .borrow()
                .iter()
                .find(|frame| frame.source_reference.cycle(db) == reference.cycle(db))
                .cloned();
            if let Some(frame) = frame {
                let source = reference.restore(db, frame.source);
                let mapped = source.apply_type_mapping_impl(db, mapping, tcx, visitor);
                return if self.probe.is_some() { ty } else { mapped };
            }
            if let Some(probe) = self.probe
                && self.outer.is_some_and(|outer| {
                    outer
                        .active
                        .borrow()
                        .iter()
                        .any(|frame| frame.source_reference.cycle(db) == reference.cycle(db))
                })
            {
                probe.changed.set(true);
            }
            return ty;
        }

        let arguments = reference
            .arguments(db)
            .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
        let reference = reference.with_arguments(db, arguments);
        match mapping {
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Restore {
                    placeholder,
                    source,
                },
            )) if reference.cycle(db) == placeholder.cycle(db) => reference
                .with_arguments(db, placeholder.project_arguments(db, arguments))
                .restore(db, *source),
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Close { cycle, placeholder },
            )) if reference.cycle(db) == *cycle => {
                let closed = placeholder.rebind_arguments(db, arguments);
                let closed = RecursiveMappingReference::new(
                    db,
                    closed.scope(db),
                    closed.index(db),
                    closed.arguments(db),
                    reference.materialization_kind(db),
                );
                self.reference_type(db, visitor.env, closed)
            }
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::BindFresh(placeholder),
            )) if reference.cycle(db) == placeholder.cycle(db) => {
                Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    placeholder.cycle(db),
                    placeholder.project_arguments(db, arguments),
                ))
            }
            TypeMapping::Materialize(kind) => self.reference_type(
                db,
                visitor.env,
                RecursiveMappingReference::new(
                    db,
                    reference.scope(db),
                    reference.index(db),
                    arguments,
                    Some(*kind),
                ),
            ),
            _ => self.reference_type(db, visitor.env, reference),
        }
    }

    fn new(source: Type<'db>, mapping: &TypeMapping<'a, 'db>) -> Self {
        let mut mappings = vec![mapping.clone()];
        let flipped = mapping.flip();
        if flipped != *mapping {
            mappings.push(flipped);
        }
        if let TypeMapping::RescopeReturnCallables(replacements) = mapping {
            mappings.push(TypeMapping::ApplySpecialization(
                ApplySpecialization::ReturnCallables(replacements),
            ));
        }
        Self {
            scope: source,
            mappings,
            active: RefCell::default(),
            next_binder: Cell::new(0),
            references: Rc::default(),
            completed: RefCell::default(),
            probe: None,
            outer: None,
            parameter_sources: FxHashMap::default(),
        }
    }

    /// Map a closed recursive body and bind the transformed recursive references.
    pub(super) fn apply(
        db: &'db dyn Db,
        source: Type<'db>,
        mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let mut context = Self::new(source, mapping);
        let analysis = context.analyze(db, source, mapping, tcx, visitor);
        if !analysis.changed {
            return source;
        }
        // A caller's recursion guard can return an unchanged unfolding containing gradual types.
        // Analyze the closed body independently before marking recursive references as materialized.
        if let TypeMapping::Materialize(kind) = mapping {
            match source {
                Type::Recursive(recursive) => {
                    return Type::Recursive(recursive.with_materialization(db, Some(*kind)));
                }
                Type::TypeAlias(alias) if alias.is_recursive(db) => {
                    return Type::TypeAlias(alias.with_materialization_kind(db, Some(*kind)));
                }
                _ => {}
            }
        }
        context.parameter_sources = analysis.parameter_sources;
        let nested_visitor = context.visitor(visitor);
        source.apply_type_mapping_impl(db, mapping, tcx, &nested_visitor)
    }

    /// Preserve unchanged applications, including constant arguments inside a growing constructor.
    fn analyze(
        &self,
        db: &'db dyn Db,
        source: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RecursiveMappingAnalysis<'db> {
        let usage = RecursiveMappingProbe::default();
        let probe = RecursiveTypeMapping {
            scope: source,
            mappings: self.mappings.clone(),
            active: RefCell::default(),
            next_binder: Cell::new(0),
            references: Rc::clone(&self.references),
            completed: RefCell::default(),
            probe: Some(&usage),
            outer: Some(self),
            parameter_sources: FxHashMap::default(),
        };
        let probe_visitor = probe.visitor(visitor);
        source.apply_type_mapping_impl(db, mapping, tcx, &probe_visitor);
        let mut checked = FxHashSet::default();
        loop {
            let previous = checked.len();
            let calls = usage.calls.borrow().clone();
            for (index, call) in calls.iter().enumerate() {
                let used = usage.nodes.borrow()[call.target].used.clone();
                for (variable, state) in used {
                    if !checked.insert((index, variable, state)) {
                        continue;
                    }
                    let Some(argument) = call
                        .arguments
                        .generic_context(db)
                        .variables(db)
                        .find(|candidate| candidate.identity(db) == variable)
                        .and_then(|variable| call.arguments.get(db, variable))
                    else {
                        continue;
                    };
                    usage.scopes.borrow_mut().clone_from(&call.scopes);
                    if argument.apply_type_mapping_impl(
                        db,
                        &probe.mappings[state],
                        tcx,
                        &probe_visitor,
                    ) != argument
                    {
                        usage.changed.set(true);
                    }
                }
            }
            if previous == checked.len() {
                break;
            }
        }
        let mut parameter_sources: FxHashMap<_, FxOrderSet<_>> = FxHashMap::default();
        if let Some(TypeMapping::RescopeReturnCallables(replacements)) = self
            .mappings
            .iter()
            .find(|mapping| matches!(mapping, TypeMapping::RescopeReturnCallables(_)))
        {
            let nodes = usage.nodes.borrow();
            let mut dependencies = Vec::new();
            for call in usage.calls.borrow().iter() {
                for (parameter, argument) in call
                    .arguments
                    .generic_context(db)
                    .variables(db)
                    .zip(call.arguments.types(db))
                {
                    for variable in argument.bound_typevars_in_annotation(db, visitor.env) {
                        if call.scopes.iter().any(|scope| {
                            nodes[*scope].parameters.is_some_and(|parameters| {
                                parameters.variables(db).any(|parameter| {
                                    parameter.identity(db) == variable.identity(db)
                                })
                            })
                        }) {
                            dependencies.push((parameter.identity(db), variable.identity(db)));
                        } else if replacements.contains_key(&variable) {
                            parameter_sources
                                .entry(parameter.identity(db))
                                .or_default()
                                .insert(variable);
                        }
                    }
                }
            }
            loop {
                let mut changed = false;
                for &(target, source) in &dependencies {
                    let sources = parameter_sources.get(&source).cloned().unwrap_or_default();
                    let target = parameter_sources.entry(target).or_default();
                    let before = target.len();
                    target.extend(sources);
                    changed |= before != target.len();
                }
                if !changed {
                    break;
                }
            }
        }
        RecursiveMappingAnalysis {
            changed: usage.changed.get(),
            parameter_sources,
        }
    }

    fn mapped_parameter(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        mapping: &TypeMapping<'_, 'db>,
    ) -> Option<Type<'db>> {
        let state = self
            .mappings
            .iter()
            .position(|candidate| candidate == mapping)?;
        self.active
            .borrow()
            .iter()
            .rev()
            .find_map(|frame| {
                frame
                    .parameter_mappings
                    .get(state)
                    .and_then(|parameters| parameters.get(db, variable))
            })
            .or_else(|| {
                self.outer
                    .and_then(|outer| outer.mapped_parameter(db, variable, mapping))
            })
    }

    /// Follow formal arguments to the function variables quantified by a returned callable.
    pub(super) fn rescoped_variables(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut ty: Type<'db>,
    ) -> FxOrderSet<BoundTypeVarInstance<'db>> {
        // The probe records parameter uses under the substitution; quantifiers are added only
        // after following arguments through all recursive calls.
        if self.probe.is_some() {
            return FxOrderSet::default();
        }
        for frame in self.active.borrow().iter().rev() {
            let restore = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Restore {
                    placeholder: frame.source_reference,
                    source: frame.source,
                },
            ));
            let visitor = ApplyTypeMappingVisitor {
                recursive_mapping: Some(self),
                ..ApplyTypeMappingVisitor::new(env)
            };
            ty = ty.apply_type_mapping_impl(db, &restore, TypeContext::default(), &visitor);
        }
        ty.bound_typevars_in_annotation(db, env)
            .into_iter()
            .flat_map(|variable| {
                self.parameter_sources
                    .get(&variable.identity(db))
                    .cloned()
                    .unwrap_or_else(|| FxOrderSet::from_iter([variable]))
            })
            .collect()
    }

    /// Intercept recursive references and formal arguments owned by this transformation.
    pub(super) fn map_type(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Type<'db>> {
        if let Type::Recursive(recursive) = ty
            && let TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Abstract {
                    constructor,
                    placeholder,
                },
            )) = mapping
            && recursive.constructor(db) == *constructor
        {
            let reference = placeholder.rebind_arguments(db, recursive.arguments(db));
            let reference = RecursiveMappingReference::new(
                db,
                reference.scope(db),
                reference.index(db),
                reference.arguments(db),
                recursive.materialization_kind(db),
            );
            return Some(self.reference_type(db, visitor.env, reference));
        }
        if let Type::RecursiveVar(variable) = ty
            && let TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Close { cycle, placeholder },
            )) = mapping
            && variable.cycle(db) == *cycle
        {
            let arguments = variable
                .arguments(db)
                .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
            return Some(self.reference_type(
                db,
                visitor.env,
                placeholder.rebind_arguments(db, arguments),
            ));
        }
        let reference = self.references.borrow().get(&ty).copied();
        if let Some(reference) = reference {
            return Some(self.map_reference(db, ty, reference, mapping, tcx, visitor));
        }
        let state = self
            .mappings
            .iter()
            .position(|candidate| candidate == mapping)?;
        if let Type::TypeVar(variable) = ty {
            if let Some(probe) = self.probe {
                for scope in probe.scopes.borrow().iter().rev() {
                    let mut nodes = probe.nodes.borrow_mut();
                    let node = &mut nodes[*scope];
                    if node.parameters.is_some_and(|parameters| {
                        parameters
                            .variables(db)
                            .any(|parameter| parameter.identity(db) == variable.identity(db))
                    }) {
                        node.used.insert((variable.identity(db), state));
                        return Some(ty);
                    }
                }
            }
            return self.mapped_parameter(db, variable, mapping);
        }
        if !matches!(ty, Type::Recursive(_) | Type::TypeAlias(_)) {
            return None;
        }
        let key = self.probe.is_none().then(|| RecursiveMappingKey {
            source: ty,
            state,
            context: tcx,
            parameters: self
                .active
                .borrow()
                .iter()
                .flat_map(|frame| frame.parameter_mappings.iter().copied())
                .collect(),
        });
        if let Some(key) = &key
            && let Some(mapped) = self.completed.borrow().get(key)
        {
            return Some(*mapped);
        }
        let mapped = self.map_alias(db, ty, mapping, tcx, visitor, state)?;
        if let Some(key) = key
            && !self.contains_reference(db, visitor.env, mapped)
        {
            self.completed.borrow_mut().insert(key, mapped);
        }
        Some(mapped)
    }

    /// Completed results can be shared only after all references to enclosing frames are closed.
    fn contains_reference(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> bool {
        struct PlaceholderSearch<'a, 'db> {
            env: &'a ProgramEnvironment<'db>,
            references: &'a FxHashMap<Type<'db>, RecursiveMappingReference<'db>>,
            found: Cell<bool>,
            seen: TypeCollector<'db>,
        }

        impl<'db> TypeVisitor<'db> for PlaceholderSearch<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.env
            }

            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                if self.found.get() {
                    return;
                }
                if self.references.contains_key(&ty) {
                    self.found.set(true);
                    return;
                }
                if let Type::RecursiveVar(variable) = ty {
                    if let Some(arguments) = variable.arguments(db) {
                        walk_specialization_types(db, arguments, self);
                    }
                    return;
                }
                walk_type_with_recursion_guard(db, ty, self, &self.seen);
            }

            fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
                // Inspect stored bodies without unfolding open recursive variables.
                self.visit_type(db, recursive.body(db));
                if let Some(arguments) = recursive.arguments(db) {
                    walk_specialization_types(db, arguments, self);
                }
            }

            fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
                if let Some(arguments) = alias.specialization(db) {
                    walk_specialization_types(db, arguments, self);
                }
            }
        }

        let references = self.references.borrow();
        let search = PlaceholderSearch {
            env,
            references: &references,
            found: Cell::new(false),
            seen: TypeCollector::default(),
        };
        search.visit_type(db, ty);
        search.found.get()
    }

    fn map_alias(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        state: usize,
    ) -> Option<Type<'db>> {
        let (source, arguments) = match ty {
            Type::Recursive(recursive) if recursive.is_cycle_seed(db) => return Some(ty),
            Type::Recursive(recursive) if recursive.may_have_unbounded_specialization(db) => (
                Type::Recursive(recursive.constructor(db)),
                recursive.arguments(db),
            ),
            Type::TypeAlias(alias)
                if matches!(ty.to_type_identity(db), TypeIdentity::GrowingTypeAlias(_)) =>
            {
                let constructor = alias
                    .unspecialized(db)
                    .apply_specialization(db, |parameters| parameters.identity_specialization(db));
                let arguments = alias.specialization(db).or_else(|| {
                    alias
                        .generic_context(db)
                        .map(|parameters| parameters.default_specialization(db, None))
                });
                (Type::TypeAlias(constructor), arguments)
            }
            Type::Recursive(_) | Type::TypeAlias(_) => (ty, None),
            _ => return None,
        };
        let materialization = match ty {
            Type::Recursive(recursive) => recursive.materialization_kind(db).map(|kind| {
                (
                    Type::Recursive(recursive.with_materialization(db, None)),
                    kind,
                )
            }),
            Type::TypeAlias(alias) => alias.materialization_kind(db).map(|kind| {
                (
                    Type::TypeAlias(alias.with_materialization_kind(db, None)),
                    kind,
                )
            }),
            _ => None,
        };
        if let Some((unmaterialized, kind)) = materialization {
            if matches!(mapping, TypeMapping::Materialize(_)) {
                return Some(ty);
            }
            // Materialize arguments at their occurrences before the next transformation.
            // Materializing a formal constructor alone would lose the effect on later arguments.
            let materialization = TypeMapping::Materialize(kind);
            let mut context = Self::new(unmaterialized, &materialization);
            context.references = Rc::clone(&self.references);
            let materialization_visitor = context.visitor(visitor);
            let materialized = unmaterialized.apply_type_mapping_impl(
                db,
                &materialization,
                tcx,
                &materialization_visitor,
            );
            return Some(materialized.apply_type_mapping_impl(db, mapping, tcx, visitor));
        }
        if let Some(probe) = self.probe {
            let mut nodes = probe.nodes.borrow_mut();
            // An applied alias can mention parameters bound by an enclosing constructor.
            // Its occurrences in different scopes must contribute to their respective owners.
            let scopes = if arguments.is_none() {
                probe
                    .scopes
                    .borrow()
                    .iter()
                    .copied()
                    .filter(|scope| nodes[*scope].parameters.is_some())
                    .collect::<Box<[_]>>()
            } else {
                Box::default()
            };
            let existing = nodes.iter().position(|node| {
                node.source == source && node.state == state && node.scopes == scopes
            });
            let index = existing.unwrap_or_else(|| {
                let index = nodes.len();
                nodes.push(RecursiveProbeNode {
                    source,
                    state,
                    scopes,
                    parameters: arguments.map(|arguments| arguments.generic_context(db)),
                    used: FxOrderSet::default(),
                });
                index
            });
            drop(nodes);
            if let Some(arguments) = arguments {
                let call = RecursiveProbeCall {
                    target: index,
                    arguments,
                    scopes: probe.scopes.borrow().clone(),
                };
                let mut calls = probe.calls.borrow_mut();
                if !calls.contains(&call) {
                    calls.push(call);
                }
            }
            if existing.is_some() {
                return Some(ty);
            }
            probe.scopes.borrow_mut().push(index);
        }
        if self.probe.is_none()
            && !self.active.borrow().is_empty()
            && !self.analyze(db, ty, mapping, tcx, visitor).changed
        {
            return Some(ty);
        }
        let active = self
            .active
            .borrow()
            .iter()
            .find(|frame| frame.source == source && frame.state == state)
            .cloned();
        let frame = active.clone().unwrap_or_else(|| {
            let parameter_mappings = arguments
                .map(|arguments| {
                    let original = arguments.generic_context(db);
                    self.mappings
                        .iter()
                        .enumerate()
                        .map(|(state, _mapping)| {
                            if self.probe.is_some() {
                                return original.identity_specialization(db);
                            }
                            let mut suffix = format!("mapping{state}");
                            while original.variables(db).any(|variable| {
                                let mapped = variable.with_name_suffix(db, &suffix);
                                original
                                    .variables(db)
                                    .any(|original| original.identity(db) == mapped.identity(db))
                            }) {
                                suffix.push('_');
                            }
                            let types = original
                                .variables(db)
                                .map(|variable| {
                                    Type::TypeVar(variable.with_name_suffix(db, &suffix))
                                })
                                .collect::<Box<[_]>>();
                            Specialization::new(db, original, types, None, None)
                        })
                        .collect::<Box<[_]>>()
                })
                .unwrap_or_default();
            let parameters = arguments.map(|arguments| {
                GenericContext::from_typevar_instances(
                    db,
                    visitor.env,
                    arguments.generic_context(db).variables(db).chain(
                        parameter_mappings
                            .iter()
                            .flat_map(|mapping| mapping.types(db))
                            .filter_map(|ty| ty.as_typevar()),
                    ),
                )
            });
            let index = self.next_binder.get();
            self.next_binder.set(index + 2);
            let source_arguments = arguments
                .map(|arguments| arguments.generic_context(db).identity_specialization(db));
            let source_reference =
                RecursiveMappingReference::new(db, self.scope, index, source_arguments, None);
            let placeholder_arguments =
                parameters.map(|parameters| parameters.identity_specialization(db));
            let placeholder = RecursiveMappingReference::new(
                db,
                self.scope,
                index + 1,
                placeholder_arguments,
                None,
            );
            RecursiveMappingFrame {
                source,
                state,
                placeholder,
                source_reference,
                parameter_mappings,
            }
        });
        let mapped_arguments = arguments
            .zip(
                frame
                    .placeholder
                    .arguments(db)
                    .map(|arguments| arguments.generic_context(db)),
            )
            .map(|(arguments, parameters)| {
                if self.probe.is_some() {
                    return arguments;
                }
                let mut mapped = arguments.types(db).to_vec();
                for mapping in &self.mappings {
                    for argument in arguments.types(db) {
                        mapped.push(argument.apply_type_mapping_impl(db, mapping, tcx, visitor));
                    }
                }
                Specialization::new(db, parameters, mapped.into_boxed_slice(), None, None)
            });
        if active.is_some() {
            return Some(self.reference_type(
                db,
                visitor.env,
                frame.placeholder.with_arguments(db, mapped_arguments),
            ));
        }
        self.active.borrow_mut().push(frame.clone());
        let node_visitor = self.visitor(visitor);
        let original = match source {
            Type::Recursive(recursive) => {
                let close = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                    RecursiveSubstitution::Close {
                        cycle: recursive.cycle(db),
                        placeholder: frame.source_reference,
                    },
                ));
                let closed = recursive.body(db).apply_type_mapping_impl(
                    db,
                    &close,
                    tcx,
                    &self.visitor(visitor),
                );
                match recursive.arguments(db) {
                    Some(arguments) => closed.apply_type_mapping_impl(
                        db,
                        &TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(
                            arguments,
                        )),
                        tcx,
                        &self.visitor(visitor),
                    ),
                    None => closed,
                }
            }
            Type::TypeAlias(alias) => {
                alias.value_type_with_recursion(db, visitor.recursion_context)
            }
            _ => source,
        };
        let mapped = original.apply_type_mapping_impl(db, mapping, tcx, &node_visitor);
        self.active.borrow_mut().pop();
        if let Some(probe) = self.probe {
            probe.scopes.borrow_mut().pop();
            if mapped != original {
                probe.changed.set(true);
            }
            return Some(ty);
        }
        if mapped == original {
            return Some(ty);
        }
        let restore = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
            RecursiveSubstitution::Restore {
                placeholder: frame.source_reference,
                source,
            },
        ));
        let mapped = mapped.apply_type_mapping_impl(db, &restore, tcx, &self.visitor(visitor));
        let (mapped, placeholder, mapped_arguments) = match mapped_arguments {
            Some(arguments) => {
                RecursiveParameters::reduce(db, self, mapped, frame.placeholder, arguments, visitor)
            }
            None => (mapped, frame.placeholder, None),
        };
        let bind = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
            RecursiveSubstitution::BindFresh(placeholder),
        ));
        let body = mapped.apply_type_mapping_impl(db, &bind, tcx, &self.visitor(visitor));
        let mapped = if body.has_unguarded_alias_cycle(db) {
            Type::divergent_alias(placeholder.cycle(db).0)
        } else if body == mapped {
            mapped
        } else {
            let definition = match source {
                Type::Recursive(recursive) => recursive.definition(db),
                Type::TypeAlias(alias) => alias.definition(db),
                _ => unreachable!("only recursive aliases have mapping frames"),
            };
            Type::Recursive(RecursiveType::new_internal(
                db,
                RecursiveOrigin::Transformed(definition),
                placeholder.cycle(db),
                body,
                placeholder.arguments(db),
                None,
            ))
        };
        Some(match mapped_arguments {
            Some(arguments) => mapped.apply_type_mapping_impl(
                db,
                &TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(arguments)),
                tcx,
                &self.visitor(visitor),
            ),
            None => mapped,
        })
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
