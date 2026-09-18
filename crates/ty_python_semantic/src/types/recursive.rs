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
//! A binder can also bind several bodies at once, as the solution of mutually recursive
//! constraints such as `T = tuple[U]` and `U = list[T]`:
//!
//! ```text
//! μ(a, b). (tuple[b], list[a])
//! ```
//!
//! Each of `T` and `U` is an *entry* of that binder, and unfolding an entry substitutes
//! entries for the variables of its body. Sharing one binder keeps the solution as large as
//! its equations; nesting one binder per variable would instead copy each body into the
//! others. Its variables are named by position. Such a binder is always closed as a whole, so
//! one never occurs open inside another, and a positional variable refers to the nearest one.
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
use std::iter;

use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::generics::{ApplySpecialization, Specialization, walk_specialization_types};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::type_alias::AliasCycleSummary;
use super::variance::{VarianceInferable, VarianceOrigin};
use super::visitor::{TypeCollector, TypeVisitor, any_over_type, walk_type_with_recursion_guard};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, GenericContext,
    MaterializationKind, Type, TypeAliasType, TypeContext, TypeMapping, UnionType, VarianceTerm,
};
use crate::{Db, FxIndexSet, Program, ProgramEnvironment};

/// A recursive variable, named by an alias's query cycle or by its position in a solution.
/// An escaping reference has no type semantics; in particular, it is neither a
/// gradual type nor an assignability operand.
/// Only recursive-type binding and substitution operations may construct or operate on recursive variables.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveVar<'db> {
    /// Refers to the nearest enclosing recursive binder of this kind.
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
            )) if self.cycle(db).binds_in(db, *recursive) => match self.cycle(db) {
                RecursiveCycle::Alias(_) => {
                    Type::Recursive(recursive.with_arguments(db, arguments))
                }
                RecursiveCycle::Equation(entry) => Type::Recursive(recursive.at(db, entry)),
            },
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
    Bind(#[get_size(ignore)] salsa::Id),
    /// Replace each placeholder of a solution with the variable of its equation.
    BindSolution(RecursiveEquations<'db>),
}

impl<'db> RecursiveSubstitution<'db> {
    /// Whether this substitution replaces the variables that `binder` binds in its own bodies,
    /// which must therefore be left alone.
    fn is_shadowed_by(self, db: &'db dyn Db, binder: RecursiveType<'db>) -> bool {
        let target = match self {
            Self::Unfold(recursive) => recursive.origin(db),
            Self::Bind(cycle) => return binder.origin(db).alias_cycle() == Some(cycle),
            Self::BindSolution(_) => return true,
        };
        match target {
            RecursiveOrigin::Alias { cycle, .. } => binder.origin(db).alias_cycle() == Some(cycle),
            // A solution is closed when it is created, and an alias's body is fixed by its
            // definition. Neither contains a variable or placeholder of another solution.
            RecursiveOrigin::Solution(_) => true,
        }
    }
}

/// Names a recursive variable and the binder that it refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecursiveCycle {
    /// An alias query. Its identity is stable across the iterations of that query.
    Alias(salsa::Id),
    /// The equation at this position of the nearest enclosing solution.
    Equation(usize),
}

impl get_size2::GetSize for RecursiveCycle {}

impl RecursiveCycle {
    fn binds_in<'db>(self, db: &'db dyn Db, binder: RecursiveType<'db>) -> bool {
        match (self, binder.origin(db)) {
            (Self::Alias(variable), RecursiveOrigin::Alias { cycle, .. }) => variable == cycle,
            (Self::Equation(_), RecursiveOrigin::Solution(_)) => true,
            _ => false,
        }
    }
}

/// Where a recursive binder was constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, salsa::SalsaValue)]
pub enum RecursiveOrigin<'db> {
    Alias {
        /// The defining symbol of the implicit alias, including for qualified references.
        definition: Definition<'db>,
        /// Names the binder and distinguishes provisional types of different alias queries.
        cycle: salsa::Id,
    },
    /// The solution of recursive type constraints, which has no defining symbol.
    Solution(Program<'db>),
}

impl get_size2::GetSize for RecursiveOrigin<'_> {}

impl RecursiveOrigin<'_> {
    fn alias_cycle(self) -> Option<salsa::Id> {
        match self {
            Self::Alias { cycle, .. } => Some(cycle),
            Self::Solution(_) => None,
        }
    }
}

/// The bodies that one binder binds together. Entries of the same binder share them.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveEquations<'db> {
    #[returns(deref)]
    bodies: Box<[Type<'db>]>,
}

impl get_size2::GetSize for RecursiveEquations<'_> {}

/// An application of a structural recursive type constructor.
/// The private body remains unspecialized; `arguments` records this application's
/// substitution for the constructor's type parameters. Unfolding replaces recursive
/// references with closed types, then applies that substitution before exposing the
/// result to ordinary type operations.
/// Use the binding operations in this module to construct recursive types.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    #[returns(copy)]
    origin: RecursiveOrigin<'db>,
    #[returns(copy)]
    equations: RecursiveEquations<'db>,
    /// The equation whose body this type unfolds to. An alias binds a single body.
    #[returns(copy)]
    entry: usize,
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
            cycle_initial=|db, id, _, ()| AliasCycleSummary::from_type(db, Type::divergent(id)),
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
            // A solution guards every variable, so only an alias's can be exposed.
            if let Some(Type::RecursiveVar(variable)) = summary.cycle
                && let RecursiveCycle::Alias(cycle) = variable.cycle(db)
            {
                summary.cycle = Some(Type::divergent(cycle));
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
        let arguments = parameters.map(|parameters| parameters.identity_specialization(db));
        let variable = RecursiveVar::new_internal(db, RecursiveCycle::Alias(cycle), arguments);
        Self::new_internal(
            db,
            RecursiveOrigin::Alias { definition, cycle },
            RecursiveEquations::new_internal(db, Box::from([Type::RecursiveVar(variable)])),
            0,
            arguments,
            None,
        )
    }

    fn body(self, db: &'db dyn Db) -> Type<'db> {
        self.equations(db).bodies(db)[self.entry(db)]
    }

    /// Another entry of this solution's binder.
    fn at(self, db: &'db dyn Db, entry: usize) -> Self {
        Self::new_internal(
            db,
            self.origin(db),
            self.equations(db),
            entry,
            None,
            self.materialization_kind(db),
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
            cycle,
        )
    }

    /// Bind references to this alias's query cycle in a closed inference result.
    /// Each bound occurrence retains its arguments and refers to this constructor's cycle.
    fn bind(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        original: Type<'db>,
        cycle: salsa::Id,
    ) -> Type<'db> {
        let body = original.apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Bind(cycle),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(env),
        );
        // Alias arguments can expose a reference without introducing a container.
        if body.has_unguarded_alias_cycle(db) {
            Type::divergent(cycle)
        } else if body == original {
            // Binding changes a closed type only by introducing references to this binder.
            body
        } else {
            Type::Recursive(Self::new_internal(
                db,
                self.origin(db),
                RecursiveEquations::new_internal(db, Box::from([body])),
                0,
                self.arguments(db),
                None,
            ))
        }
    }

    fn with_arguments(self, db: &'db dyn Db, arguments: Option<Specialization<'db>>) -> Self {
        Self::new_internal(
            db,
            self.origin(db),
            self.equations(db),
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
            self.equations(db),
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

    /// The defining symbol of the source alias. The solution of recursive constraints has none.
    pub(super) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        match self.origin(db) {
            RecursiveOrigin::Alias { definition, .. } => Some(definition),
            RecursiveOrigin::Solution(_) => None,
        }
    }

    /// The source alias's definition and its declared name, which all specializations of this
    /// constructor share.
    pub(super) fn alias(self, db: &'db dyn Db) -> Option<(Definition<'db>, &'db str)> {
        let definition = self.definition(db)?;
        // Qualified uses retain the original declaration's symbol, not the access expression.
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
            RecursiveOrigin::Solution(program) => ProgramEnvironment::from_program(program),
        }
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
            )) if matches!(self.origin(db), RecursiveOrigin::Alias { cycle: own, .. } if own == *cycle)
                && self.materialization_kind(db).is_none() =>
            {
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    RecursiveCycle::Alias(*cycle),
                    arguments,
                ))
            }
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::BindSolution(placeholders),
            )) if self.equations(db) == *placeholders && self.is_placeholder(db) => {
                Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    RecursiveCycle::Equation(self.entry(db)),
                    None,
                ))
            }
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(substitution)) => {
                // This binder shadows the target in its bodies, but not in its arguments.
                let equations = if substitution.is_shadowed_by(db, self) {
                    self.equations(db)
                } else {
                    let bodies = self.equations(db).bodies(db).iter();
                    RecursiveEquations::new_internal(
                        db,
                        bodies
                            .map(|body| body.apply_type_mapping_impl(db, mapping, tcx, visitor))
                            .collect::<Box<[_]>>(),
                    )
                };
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::Recursive(Self::new_internal(
                    db,
                    self.origin(db),
                    equations,
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
                if matches!(self.origin(db), RecursiveOrigin::Solution(_)) =>
            {
                // A solution has no parameters: free variables occur in its bodies instead.
                self.map_solution(db, mapping, tcx, visitor)
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

    /// A closed stand-in for the entry at the same position of a solution being formed. Ordinary
    /// type operations can inspect the types that contain it, and `BindSolution` then replaces
    /// it with that entry's variable. It unfolds to itself, like the `μa. a` of an alias query.
    fn placeholders(db: &'db dyn Db, program: Program<'db>, len: usize) -> Self {
        let variables = (0..len).map(|entry| {
            Type::RecursiveVar(RecursiveVar::new_internal(
                db,
                RecursiveCycle::Equation(entry),
                None,
            ))
        });
        Self::new_internal(
            db,
            RecursiveOrigin::Solution(program),
            RecursiveEquations::new_internal(db, variables.collect::<Box<[_]>>()),
            0,
            None,
            None,
        )
    }

    fn is_placeholder(self, db: &'db dyn Db) -> bool {
        self.arguments(db).is_none()
            && self.materialization_kind(db).is_none()
            && matches!(self.body(db), Type::RecursiveVar(variable)
                if variable.cycle(db) == RecursiveCycle::Equation(self.entry(db)))
    }

    /// Binds the placeholders in `bodies` under one binder, and returns its entries.
    fn bind_solution(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        placeholders: Self,
        bodies: impl Iterator<Item = Type<'db>>,
    ) -> impl Iterator<Item = Self> {
        let mapping = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
            RecursiveSubstitution::BindSolution(placeholders.equations(db)),
        ));
        let visitor = ApplyTypeMappingVisitor::new(env);
        let bodies: Box<[_]> = bodies
            .map(|body| {
                body.apply_type_mapping_impl(db, &mapping, TypeContext::default(), &visitor)
            })
            .collect();
        let equations = RecursiveEquations::new_internal(db, bodies);
        (0..equations.bodies(db).len()).map(move |entry| {
            Self::new_internal(db, placeholders.origin(db), equations, entry, None, None)
        })
    }

    /// Solves the equations `variable = body`, whose bodies refer to each other's variables, by
    /// closing them into recursive types. Returns `None` if some reference is not guarded by a
    /// type constructor and cannot be eliminated, as in `T = T & tuple[T]`: unfolding it would
    /// never reach a type to inspect.
    pub(super) fn from_equations(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        equations: &[(BoundTypeVarInstance<'db>, Type<'db>)],
    ) -> Option<Vec<Type<'db>>> {
        let UnguardedReferences { bodies, equal_to } =
            Self::inline_unguarded_references(db, env, equations)?;

        // Refer to variables that are equal by a single one of them, and solve for that one.
        let is_retained = |entry: &usize| equal_to[*entry] == *entry;
        let renamed: Vec<_> = (0..equations.len())
            .filter(|entry| !is_retained(entry))
            .collect();
        let bodies = if renamed.is_empty() {
            bodies
        } else {
            let context = GenericContext::from_typevar_instances(
                db,
                env,
                renamed.iter().map(|entry| equations[*entry].0),
            );
            let names: Vec<_> = renamed
                .iter()
                .map(|entry| Type::TypeVar(equations[equal_to[*entry]].0))
                .collect();
            let renaming = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
                generic_context: context,
                types: &names,
                skip: None,
            });
            bodies
                .iter()
                .map(|body| body.apply_type_mapping(db, env, &renaming, TypeContext::default()))
                .collect()
        };
        let retained: Vec<_> = (0..equations.len()).filter(is_retained).collect();
        let variables: Vec<_> = retained.iter().map(|entry| equations[*entry].0).collect();
        let bodies: Vec<_> = retained.iter().map(|entry| bodies[*entry]).collect();

        let solution = Self::close_least_unrolled(db, env, &variables, &bodies)?;
        (0..equations.len())
            .map(|entry| {
                let position = retained
                    .iter()
                    .position(|other| *other == equal_to[entry])?;
                Some(solution[position])
            })
            .collect()
    }

    /// Closes `variables[i] = bodies[i]`, preferring the solution without unrolled bounds.
    fn close_least_unrolled(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
        bodies: &[Type<'db>],
    ) -> Option<Vec<Type<'db>>> {
        // The bounds of a variable include those that transitivity derives by replacing other
        // variables with their own bounds: `A ≤ tuple[B]` and `B ≤ list[A]` also give
        // `A ≤ tuple[list[A]]`. Each of them denotes the same solution, so the least unrolled
        // one is sufficient, while keeping all of them would multiply the size of the solution.
        // Bounds that differ in any other way fail the verification of the smaller solution.
        let least_unrolled: Vec<_> = bodies
            .iter()
            .map(|body| body.least_unrolled(db, env, variables))
            .collect();
        if least_unrolled != bodies
            && let Some(solution) = Self::close(db, env, variables, &least_unrolled, bodies)
        {
            return Some(solution);
        }
        Self::close(db, env, variables, bodies, bodies)
    }

    /// Closes `variables[i] = bodies[i]` into recursive types, and returns them if they solve
    /// `variables[i] = equations[i]`.
    fn close(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
        bodies: &[Type<'db>],
        equations: &[Type<'db>],
    ) -> Option<Vec<Type<'db>>> {
        let placeholders = Self::placeholders(db, env.program(db), variables.len());
        let context = GenericContext::from_typevar_instances(db, env, variables.iter().copied());
        let entries: Vec<_> = (0..variables.len())
            .map(|entry| Type::Recursive(placeholders.at(db, entry)))
            .collect();
        let substitution = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
            generic_context: context,
            types: &entries,
            skip: None,
        });
        let closed: Vec<_> = bodies
            .iter()
            .map(|body| body.apply_type_mapping(db, env, &substitution, TypeContext::default()))
            .collect();
        // Eliminating unguarded references can leave no reference to bind: `T = U | int`,
        // `U = T` has the solution `T = U = int`.
        if closed == bodies {
            return Some(closed);
        }
        let solution: Vec<_> =
            Self::bind_solution(db, env, placeholders, closed.into_iter()).collect();
        // An alias can still expose one of its arguments without a constructor.
        if solution
            .iter()
            .any(|recursive| recursive.body(db).has_unguarded_alias_cycle(db))
        {
            return None;
        }

        // Substituting a type is not always structural: `type[T]` becomes the meta-type of what
        // replaces `T`, which is meaningless for a placeholder. Such an equation is not solved
        // by the types above. They solve it if each of them is the body of its equation with
        // the solution substituted for the variables. Normalization can write that body
        // differently from the unfolding, as for the intersection in `T = tuple[T & ~E]`.
        let types: Vec<_> = solution.iter().copied().map(Type::Recursive).collect();
        let substitution = TypeMapping::ApplySpecialization(ApplySpecialization::Partial {
            generic_context: context,
            types: &types,
            skip: None,
        });
        equations
            .iter()
            .zip(&solution)
            .all(|(equation, recursive)| {
                let substituted =
                    equation.apply_type_mapping(db, env, &substitution, TypeContext::default());
                substituted == recursive.unfold(db, env).into_type()
                    || substituted.is_equivalent_to(db, env, Type::Recursive(*recursive))
            })
            .then_some(types)
    }

    /// Replaces each variable that a body exposes outside a type constructor, as `U` in
    /// `T = U | int`, with the body of its equation. Every remaining reference is then guarded.
    ///
    /// Variables that expose each other, as in `T = U | int`, `U = T | str`, contain each other
    /// and are therefore equal. The least type that solves their equations is the union of what
    /// each of them contains besides the others: `T = U = int | str`.
    ///
    /// Returns `None` if an unguarded reference passes through a type alias, if variables
    /// expose each other anywhere but in a union, or if they contain nothing else, as in
    /// `T = U`, `U = T`, which every type solves.
    fn inline_unguarded_references(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        equations: &[(BoundTypeVarInstance<'db>, Type<'db>)],
    ) -> Option<UnguardedReferences<'db>> {
        let count = equations.len();
        // The equations whose variables each body exposes.
        let exposed: Vec<Vec<usize>> = equations
            .iter()
            .map(|(_, body)| {
                body.unguarded_typevars(db)
                    .iter()
                    .filter_map(|variable| {
                        equations
                            .iter()
                            .position(|(other, _)| other.is_same_typevar_as(db, *variable))
                    })
                    .collect()
            })
            .collect();
        let mut references = UnguardedReferences {
            bodies: equations.iter().map(|(_, body)| *body).collect(),
            equal_to: (0..count).collect(),
        };
        if exposed.iter().all(Vec::is_empty) {
            return Some(references);
        }

        // Whether following exposed variables leads from one equation to another. There are
        // only as many equations as mutually dependent type variables in one call.
        let mut reaches = vec![vec![false; count]; count];
        for (from, exposed) in exposed.iter().enumerate() {
            for to in exposed {
                reaches[from][*to] = true;
            }
        }
        for through in 0..count {
            for from in 0..count {
                for to in 0..count {
                    reaches[from][to] |= reaches[from][through] && reaches[through][to];
                }
            }
        }
        let exposing_each_other = |entry: usize| -> Vec<usize> {
            (0..count)
                .filter(|other| {
                    *other == entry || (reaches[entry][*other] && reaches[*other][entry])
                })
                .collect()
        };

        // Solve each group of variables once everything else that it exposes is solved. Such a
        // group always exists, since the groups do not expose each other mutually.
        let mut is_solved = vec![false; count];
        while let Some(group) = (0..count)
            .filter(|entry| !is_solved[*entry])
            .map(exposing_each_other)
            .find(|group| {
                group.iter().all(|member| {
                    exposed[*member]
                        .iter()
                        .all(|other| is_solved[*other] || group.contains(other))
                })
            })
        {
            let contents: Option<Vec<_>> = group
                .iter()
                .map(|member| {
                    let replacements: Vec<_> = exposed[*member]
                        .iter()
                        .map(|other| {
                            let replacement =
                                (!group.contains(other)).then(|| references.bodies[*other]);
                            (equations[*other].0, replacement)
                        })
                        .collect();
                    equations[*member]
                        .1
                        .replace_unguarded_typevars(db, env, &replacements)
                })
                .collect();
            let contents = contents?;
            let first = *group.first()?;
            if reaches[first][first] {
                let body = UnionType::from_elements(db, env, contents);
                if body.is_never() {
                    return None;
                }
                for member in &group {
                    references.bodies[*member] = body;
                    references.equal_to[*member] = first;
                }
            } else {
                references.bodies[first] = *contents.first()?;
            }
            for member in group {
                is_solved[member] = true;
            }
        }
        Some(references)
    }

    /// This solution, followed by every other recursive solution that its unfolding refers to,
    /// directly or through those solutions, in the order in which they are first reached.
    ///
    /// Displaying each of them once keeps the display as large as the equations. Writing a
    /// solution out at every reference would instead repeat it for each path that leads there,
    /// which is exponential for `A = tuple[A, B, B]`, `B = tuple[B, C, C]`, and so on.
    pub(super) fn members(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> FxIndexSet<Self> {
        let mut members = FxIndexSet::from_iter([self]);
        let mut next = 0;
        while let Some(member) = members.get_index(next).copied() {
            let references = SolutionReferences {
                env,
                references: RefCell::default(),
                seen: TypeCollector::default(),
            };
            references.visit_type(db, member.unfold(db, env).into_type());
            members.extend(references.references.into_inner());
            next += 1;
        }
        members
    }

    /// Maps the free variables of a solution, which occur in its bodies. The bodies are closed
    /// with placeholders while they are mapped, and bound again afterwards.
    fn map_solution(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if self.is_placeholder(db) {
            return Type::Recursive(self);
        }
        visitor.visit(db, Type::Recursive(self), mapping, || {
            let RecursiveOrigin::Solution(program) = self.origin(db) else {
                return Type::Recursive(self);
            };
            let bodies = self.equations(db).bodies(db);
            let placeholders = Self::placeholders(db, program, bodies.len());
            let close = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Unfold(placeholders),
            ));
            let mut changed = false;
            let mapped: Vec<_> = bodies
                .iter()
                .map(|body| {
                    let closed = body.apply_type_mapping_impl(
                        db,
                        &close,
                        TypeContext::default(),
                        &ApplyTypeMappingVisitor::new(visitor.env),
                    );
                    let mapped = closed.apply_type_mapping_impl(db, mapping, tcx, visitor);
                    changed |= mapped != closed;
                    mapped
                })
                .collect();
            if !changed {
                return Type::Recursive(self);
            }
            let entry = Self::bind_solution(db, visitor.env, placeholders, mapped.into_iter())
                .nth(self.entry(db));
            entry.map_or(Type::Recursive(self), |entry| {
                Type::Recursive(entry.with_materialization(db, self.materialization_kind(db)))
            })
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

/// The bodies of a group of equations once no variable of the group occurs outside of a type
/// constructor.
struct UnguardedReferences<'db> {
    bodies: Vec<Type<'db>>,
    /// For each equation, the first of the equations whose variables are equal to its own.
    equal_to: Vec<usize>,
}

/// Collects the recursive solutions that a type refers to, without entering them.
struct SolutionReferences<'env, 'db> {
    env: &'env ProgramEnvironment<'db>,
    references: RefCell<Vec<RecursiveType<'db>>>,
    seen: TypeCollector<'db>,
}

impl<'db> TypeVisitor<'db> for SolutionReferences<'_, 'db> {
    fn program_environment(&self) -> &ProgramEnvironment<'db> {
        self.env
    }

    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        walk_type_with_recursion_guard(db, ty, self, &self.seen);
    }

    fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
        // The display includes the arguments, not the alias's body.
        if let Some(arguments) = alias.specialization(db) {
            walk_specialization_types(db, arguments, self);
        }
    }

    fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
        if recursive.definition(db).is_none() {
            self.references.borrow_mut().push(recursive);
        } else if let Some(arguments) = recursive.arguments(db) {
            // An alias is displayed by its name and arguments.
            walk_specialization_types(db, arguments, self);
        }
    }
}

impl<'db> Type<'db> {
    /// Drops the bounds in the body of an equation that can be unrollings of another bound.
    ///
    /// Among the elements of a union, or the positive elements of an intersection, that mention
    /// one of `variables`, only the smallest is retained: an unrolling replaces a variable with
    /// a larger type, so it is larger than the bound it was derived from.
    fn least_unrolled(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: &[BoundTypeVarInstance<'db>],
    ) -> Type<'db> {
        // The number of types nested in `ty`, if one of them is one of the variables.
        let recursive_size = |ty: &Type<'db>| {
            let size = Cell::new(0usize);
            let is_recursive = Cell::new(false);
            any_over_type(db, env, *ty, false, |nested| {
                size.set(size.get() + 1);
                if let Some(typevar) = nested.as_typevar()
                    && variables
                        .iter()
                        .any(|variable| variable.is_same_typevar_as(db, typevar))
                {
                    is_recursive.set(true);
                }
                false
            });
            is_recursive.get().then_some(size.get())
        };
        let retained = |elements: &[Type<'db>]| {
            let sizes: Vec<_> = elements.iter().map(recursive_size).collect();
            let smallest = sizes.iter().flatten().min().copied();
            let least_unrolled = sizes
                .iter()
                .position(|size| size.is_some() && *size == smallest);
            (0..sizes.len())
                .map(move |index| sizes[index].is_none() || Some(index) == least_unrolled)
        };
        match self {
            Type::Union(union) => {
                let elements: Vec<_> = union
                    .elements(db)
                    .iter()
                    .map(|element| element.least_unrolled(db, env, variables))
                    .collect();
                let retained = retained(&elements);
                UnionType::from_elements(
                    db,
                    env,
                    iter::zip(&elements, retained)
                        .filter_map(|(element, retained)| retained.then_some(*element)),
                )
            }
            Type::Intersection(intersection) => {
                let positive: Vec<_> = intersection.iter_positive(db).collect();
                let mut retained = retained(&positive);
                intersection.map_positive(db, env, |element| {
                    if retained.next() == Some(true) {
                        *element
                    } else {
                        Type::object()
                    }
                })
            }
            _ => self,
        }
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
