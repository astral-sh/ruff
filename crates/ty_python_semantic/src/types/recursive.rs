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
//! Inferred lambda returns can have the same structure. For `node = lambda: (1, node)`,
//! the lambda's return type is:
//!
//! ```text
//! μR. tuple[Literal[1], () -> R]
//! ```
//!
//! The lambda retains its source identity; this binder describes its recursive return type.
//! Captured type variables become parameters of the recursive constructor.
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

use std::cell::RefCell;

use rustc_hash::{FxHashMap, FxHashSet};
use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::callable::LambdaSignature;
use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::generics::{ApplySpecialization, Specialization};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::tuple::{Tuple, TupleSpec, VariableSegment};
use super::type_alias::AliasCycleSummary;
use super::typevar::BoundTypeVarInstance;
use super::variance::{VarianceInferable, VarianceOrigin};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, GenericContext, MaterializationKind,
    PromotionKind, PromotionMode, Type, TypeContext, TypeMapping, TypeVarVariance, VarianceTerm,
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
    /// Transformations belong to this occurrence, not to the binder's body. Binding must
    /// retain them without copying a previous approximation of the transformed body.
    #[returns(copy)]
    materialization: Option<MaterializationKind>,
    #[returns(copy)]
    promotion: Option<RecursivePromotion<'db>>,
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
            )) if self.cycle(db) == recursive.cycle(db) => Type::Recursive(
                recursive
                    .with_arguments(db, arguments)
                    .with_materialization(db, self.materialization(db))
                    .with_promotion(db, self.promotion(db)),
            ),
            TypeMapping::ApplyRecursiveSubstitution(_) => Type::RecursiveVar(Self::new_internal(
                db,
                self.cycle(db),
                arguments,
                self.materialization(db),
                self.promotion(db),
            )),
            _ => unreachable!("semantic operation on an unbound recursive variable"),
        }
    }
}

/// A structural substitution that only the recursive-type binder can construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub struct RecursiveMapping<'db>(RecursiveSubstitution<'db>);

impl<'db> RecursiveMapping<'db> {
    /// Substitute constructor arguments without starting comparisons that can unfold it again.
    /// The body is closed, so ordinary specialization handles parameters and occurrence metadata;
    /// set reconstruction remains structural until an operation consumes the result.
    fn specialize(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        body: Type<'db>,
        arguments: Specialization<'db>,
    ) -> Type<'db> {
        let specialization = ApplySpecialization::TypeAlias(arguments);
        let mapping = match arguments.materialization_kind(db) {
            None => TypeMapping::ApplySpecialization(specialization),
            Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            },
        };
        let visitor = ApplyTypeMappingVisitor::new(env).for_recursive_unfolding();
        body.apply_type_mapping_impl(db, &mapping, TypeContext::default(), &visitor)
    }
}

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

/// Identifies an inference query and names its recursive binder and variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecursiveCycle(salsa::Id);

impl get_size2::GetSize for RecursiveCycle {}

/// The source inputs of a recursive type constructor, independent of its inferred body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum RecursiveOrigin<'db> {
    Alias(Definition<'db>),
    Lambda(LambdaSignature<'db>),
}

impl<'db> RecursiveOrigin<'db> {
    fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        match self {
            Self::Alias(definition) => ProgramEnvironment::from_definition(definition),
            Self::Lambda(lambda) => ProgramEnvironment::from_scope(lambda.scope(db)),
        }
    }
}

/// Distinct inputs for occurrences that promotion visits with opposite polarities, or skips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub enum RecursivePromotionPosition {
    Positive,
    Negative,
    Unchanged,
}

impl RecursivePromotionPosition {
    const ALL: [Self; 3] = [Self::Positive, Self::Negative, Self::Unchanged];
}

/// The identity of a constructor-local parameter, independent of the source variable's bounds.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursivePromotionBinding<'db> {
    #[returns(copy)]
    pub(super) lambda: LambdaSignature<'db>,
    #[returns(copy)]
    source: BoundTypeVarIdentity<'db>,
    #[returns(copy)]
    position: RecursivePromotionPosition,
}

impl<'db> RecursivePromotionBinding<'db> {
    pub(super) fn is_source_variable(
        self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> bool {
        let identity = variable.identity(db);
        let identity = if identity.is_paramspec(db) {
            identity.without_paramspec_attr(db)
        } else {
            identity
        };
        self.source(db) == identity
    }

    pub(super) fn source_variable(self, db: &'db dyn Db) -> Option<BoundTypeVarInstance<'db>> {
        self.lambda(db)
            .captured_typevars(db)?
            .variables(db)
            .find(|variable| self.is_source_variable(db, *variable))
    }
}

impl get_size2::GetSize for RecursivePromotionBinding<'_> {}

/// Promotion of a constructor's fixed structure is finite. Its arguments retain the values
/// computed at each application, so a later specialization does not move before promotion.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue,
)]
pub struct RecursivePromotions(u8);

impl RecursivePromotions {
    fn with(self, mode: PromotionMode, kind: PromotionKind) -> Self {
        let bit = match (mode, kind) {
            (PromotionMode::On, PromotionKind::Regular) => 1,
            (PromotionMode::Off, PromotionKind::Regular) => 2,
            (PromotionMode::On, PromotionKind::ClassLiteralsOnly) => 4,
            (PromotionMode::Off, PromotionKind::ClassLiteralsOnly) => 8,
            (_, PromotionKind::SingletonsOnly) => 0,
        };
        Self(self.0 | bit)
    }

    fn iter(self) -> impl Iterator<Item = (PromotionMode, PromotionKind)> {
        [
            (PromotionMode::On, PromotionKind::Regular),
            (PromotionMode::Off, PromotionKind::Regular),
            (PromotionMode::On, PromotionKind::ClassLiteralsOnly),
            (PromotionMode::Off, PromotionKind::ClassLiteralsOnly),
        ]
        .into_iter()
        .filter(move |(mode, kind)| self.0 & Self::default().with(*mode, *kind).0 != 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct RecursivePromotion<'db> {
    source: LambdaSignature<'db>,
    promotions: RecursivePromotions,
}

impl<'db> RecursivePromotion<'db> {
    /// The regular promotion that determines which negative intersections survive.
    pub(super) fn regular_mode(self) -> Option<PromotionMode> {
        if self.promotions.0 & 1 != 0 {
            Some(PromotionMode::On)
        } else if self.promotions.0 & 2 != 0 {
            Some(PromotionMode::Off)
        } else {
            None
        }
    }

    pub(super) fn exposed_typevar(
        self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        negative: bool,
    ) -> BoundTypeVarInstance<'db> {
        RecursivePromotionContext {
            source: self.source,
            mode: PromotionMode::On,
        }
        .variable(
            db,
            variable,
            if negative {
                PromotionMode::Off
            } else {
                PromotionMode::On
            },
        )
        .unwrap_or(variable)
    }
}

/// Lift free parameters while transforming a closed constructor body. The synthetic parameters
/// are ordinary closed types; recursive variables remain private to binding and unfolding.
pub(super) struct RecursivePromotionContext<'db> {
    source: LambdaSignature<'db>,
    mode: PromotionMode,
}

impl<'db> RecursivePromotionContext<'db> {
    pub(super) fn variable(
        &self,
        db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
        mode: PromotionMode,
    ) -> Option<BoundTypeVarInstance<'db>> {
        let identity = |variable: BoundTypeVarInstance<'db>| {
            let identity = variable.identity(db);
            if identity.is_paramspec(db) {
                identity.without_paramspec_attr(db)
            } else {
                identity
            }
        };
        self.source
            .captured_typevars(db)?
            .variables(db)
            .find(|candidate| identity(*candidate) == identity(variable))?;
        Some(variable.recursive_promotion_slot(
            db,
            self.source,
            if self.mode == mode {
                RecursivePromotionPosition::Positive
            } else {
                RecursivePromotionPosition::Negative
            },
        ))
    }

    pub(super) fn unchanged(&self, db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
        let Some(parameters) = self.source.captured_typevars(db) else {
            return ty;
        };
        let specialization = Specialization::new(
            db,
            parameters,
            parameters
                .variables(db)
                .map(|variable| {
                    Type::TypeVar(variable.recursive_promotion_slot(
                        db,
                        self.source,
                        RecursivePromotionPosition::Unchanged,
                    ))
                })
                .collect::<Box<[_]>>(),
            None,
            None,
        );
        let env = ProgramEnvironment::from_scope(self.source.scope(db));
        RecursiveMapping::specialize(db, &env, ty, specialization)
    }
}

/// An application of a structural recursive type constructor.
/// The private body remains unspecialized; `arguments` records this application's
/// substitution for the constructor's type parameters. Unfolding replaces recursive
/// references with closed types, then applies that substitution before exposing the
/// result to ordinary type operations.
/// Use the binding operations in this module to construct recursive types.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct RecursiveType<'db> {
    /// The alias declaration or lambda inputs that produced this constructor.
    #[returns(copy)]
    pub(super) origin: RecursiveOrigin<'db>,
    /// Names the binder and distinguishes provisional types of different inference queries.
    #[returns(copy)]
    cycle: RecursiveCycle,
    #[returns(copy)]
    body: Type<'db>,
    /// The actual arguments for this application, which may themselves contain type variables.
    /// They are applied when unfolding; the stored body remains unspecialized.
    #[returns(copy)]
    pub(super) arguments: Option<Specialization<'db>>,
    /// The lazy materialization applied to this recursive type, if any.
    #[returns(copy)]
    pub(super) materialization_kind: Option<MaterializationKind>,
    /// Promotion retains the original binder and stores already-transformed argument values.
    /// Its finite body is transformed only after unfolding has closed all recursive variables.
    #[returns(copy)]
    promotion: Option<RecursivePromotion<'db>>,
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
            if let Some(promotion) = recursive.promotion(db) {
                summary = summary.promoted(db, promotion);
            }
            // Nested bodies can refer to an enclosing binder. Close only the cycle marker,
            // so recovery never exposes an unbound variable as a standalone type.
            summary.map_cycles(|ty| match ty {
                Type::RecursiveVar(variable) => Type::divergent_alias(variable.cycle(db).0),
                _ => ty,
            });
            summary
        }

        cycle_summary_impl(db, self.constructor(db), ())
    }

    /// Seed a query cycle with `μa. a`, using the same identity for binder and variable.
    pub(super) fn initial(
        db: &'db dyn Db,
        origin: RecursiveOrigin<'db>,
        cycle: salsa::Id,
        parameters: Option<GenericContext<'db>>,
    ) -> Self {
        let cycle = RecursiveCycle(cycle);
        let arguments = parameters.map(|parameters| parameters.identity_specialization(db));
        Self::new_internal(
            db,
            origin,
            cycle,
            Type::RecursiveVar(RecursiveVar::new_internal(db, cycle, arguments, None, None)),
            arguments,
            None,
            None,
        )
    }

    /// Close recursive occurrences after inferring a constructor expression.
    pub(super) fn recover(
        db: &'db dyn Db,
        origin: RecursiveOrigin<'db>,
        cycle: salsa::Id,
        parameters: Option<GenericContext<'db>>,
        result: Type<'db>,
    ) -> Type<'db> {
        // Shared dependencies can still contain older iterations of this query. They refer
        // to the same source even when their provisional bodies differ. Derive the binder
        // from the query inputs: an earlier result might not expose recursive references yet.
        let env = origin.environment(db);
        Self::initial(db, origin, cycle, parameters).bind(db, &env, result)
    }

    /// Whether this is the provisional `μa. a` used before a query has inferred its body.
    pub(super) fn is_initial(self, db: &'db dyn Db) -> bool {
        matches!(self.body(db), Type::RecursiveVar(variable) if variable.cycle(db) == self.cycle(db))
    }

    /// Bind references to this query cycle in a closed inference result.
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
        // A foreign constructor can still be waiting for its source query. If binding
        // introduces no references to this lambda, keep that closed dependency intact.
        if body == original && matches!(self.origin(db), RecursiveOrigin::Lambda(_)) {
            return body;
        }
        // Alias arguments can expose a reference without introducing a container.
        if body.has_unguarded_alias_cycle(db) {
            match self.origin(db) {
                RecursiveOrigin::Alias(_) => Type::divergent_alias(self.cycle(db).0),
                RecursiveOrigin::Lambda(_) => Type::divergent(self.cycle(db).0),
            }
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
                self.promotion(db),
            ))
        }
    }

    fn with_arguments(self, db: &'db dyn Db, arguments: Option<Specialization<'db>>) -> Self {
        Self::new_internal(
            db,
            self.origin(db),
            self.cycle(db),
            self.body(db),
            arguments,
            self.materialization_kind(db),
            self.promotion(db),
        )
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        _nested: bool,
    ) -> Option<Self> {
        // The body already binds its recursion. A provisional captured argument must not
        // erase that finite recursive reference, so normalize each argument independently.
        let arguments = match self.arguments(db) {
            Some(arguments) => Some(arguments.recursive_type_normalized_impl(db, env, div, false)?),
            None => None,
        };
        Some(self.with_arguments(db, arguments))
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
            self.promotion(db),
        )
    }

    fn with_promotion(self, db: &'db dyn Db, promotion: Option<RecursivePromotion<'db>>) -> Self {
        Self::new_internal(
            db,
            self.origin(db),
            self.cycle(db),
            self.body(db),
            self.arguments(db),
            self.materialization_kind(db),
            promotion,
        )
    }

    /// Parameters bound by this recursive type constructor.
    pub(super) fn parameters(self, db: &'db dyn Db) -> Option<GenericContext<'db>> {
        self.arguments(db)
            .map(|arguments| arguments.generic_context(db))
    }

    pub(super) fn promotion_state(self, db: &'db dyn Db) -> Option<RecursivePromotions> {
        self.promotion(db).map(|promotion| promotion.promotions)
    }

    fn promote(
        self,
        db: &'db dyn Db,
        mode: PromotionMode,
        kind: PromotionKind,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if kind == PromotionKind::SingletonsOnly {
            return Type::Recursive(self);
        }
        let (source, previous) = match self.promotion(db) {
            Some(promotion) => (promotion.source, promotion.promotions),
            None => match self.origin(db) {
                RecursiveOrigin::Lambda(source) => (source, RecursivePromotions::default()),
                RecursiveOrigin::Alias(_) => return Type::Recursive(self),
            },
        };
        let promotions = previous.with(mode, kind);
        let types = source.captured_typevars(db).map(|parameters| {
            parameters
                .variables(db)
                .flat_map(|variable| {
                    RecursivePromotionPosition::ALL.map(|position| {
                        let parameter = if self.promotion(db).is_some() {
                            variable.recursive_promotion_slot(db, source, position)
                        } else {
                            variable
                        };
                        let argument = self
                            .arguments(db)
                            .and_then(|arguments| arguments.get(db, parameter))
                            .unwrap_or(Type::TypeVar(parameter));
                        match position {
                            RecursivePromotionPosition::Positive => argument
                                .apply_type_mapping_impl(
                                    db,
                                    &TypeMapping::Promote(mode, kind),
                                    tcx,
                                    visitor,
                                ),
                            RecursivePromotionPosition::Negative => argument
                                .apply_type_mapping_impl(
                                    db,
                                    &TypeMapping::Promote(mode.flip(), kind),
                                    tcx,
                                    visitor,
                                ),
                            RecursivePromotionPosition::Unchanged => visitor
                                .recursive_promotion
                                .map_or(argument, |context| context.unchanged(db, argument)),
                        }
                    })
                })
                .collect::<Box<[_]>>()
        });
        let arguments = promotion_parameters(db, source)
            .zip(types)
            .map(|(parameters, types)| {
                Specialization::new(
                    db,
                    parameters,
                    types,
                    self.arguments(db)
                        .and_then(|arguments| arguments.materialization_kind(db)),
                    None,
                )
            });
        Type::Recursive(
            self.with_arguments(db, arguments)
                .with_promotion(db, Some(RecursivePromotion { source, promotions })),
        )
    }

    /// The source alias's definition and name, if this binder comes from an alias.
    pub(super) fn alias(self, db: &'db dyn Db) -> Option<(Definition<'db>, &'db str)> {
        let RecursiveOrigin::Alias(definition) = self.origin(db) else {
            return None;
        };
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

    /// The constructor whose body the binder stores, before application transformations.
    fn original_constructor(self, db: &'db dyn Db) -> Self {
        let parameters = match self.promotion(db) {
            Some(promotion) => promotion.source.captured_typevars(db),
            None => self.parameters(db),
        };
        self.with_promotion(db, None)
            .with_materialization(db, None)
            .with_arguments(
                db,
                parameters.map(|parameters| parameters.identity_specialization(db)),
            )
    }

    /// The program in which the recursive type's body was constructed.
    pub fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        self.origin(db).environment(db)
    }

    /// Substitute closed types for references before exposing the body.
    ///
    /// Report whether unfolding returns exactly `Type::Recursive(self)`. An unfolded
    /// type can still contain recursive references, so callers must retain their recursion guards.
    pub fn unfold(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> UnfoldResult<'db> {
        self.unfold_with_materialization_visitor(db, &ApplyTypeMappingVisitor::new(env))
    }

    /// Retain the consuming operation's context while materializing an unfolded body.
    pub(super) fn unfold_with_materialization_visitor(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> UnfoldResult<'db> {
        // A growing specialization cannot converge by repeating the same query key. Materialize
        // its closed unfolding directly, under the caller's recursion guard, instead.
        let unfolded = if self.materialization_kind(db).is_some()
            && !self.may_have_unbounded_specialization(db)
        {
            materialized_unfold(db, self)
        } else {
            let unfolded = self.unfolded_body(db);
            match self.materialization_kind(db) {
                Some(kind) => {
                    // Materialization uses a separate mapping cache while retaining the caller's
                    // recursion context.
                    let visitor = visitor.for_recursive_unfolding();
                    unfolded.apply_type_mapping_impl(
                        db,
                        &TypeMapping::Materialize(kind),
                        TypeContext::default(),
                        &visitor,
                    )
                }
                None => unfolded,
            }
        };
        if unfolded == Type::Recursive(self) {
            UnfoldResult::Unchanged(self)
        } else {
            UnfoldResult::Unfolded(unfolded)
        }
    }

    /// Close and specialize the body before materialization.
    fn unfolded_body(self, db: &'db dyn Db) -> Type<'db> {
        let env = self.environment(db);
        let unfolded = self.constructor(db).unfolded_constructor_body(db);
        match self
            .arguments(db)
            .and_then(|arguments| arguments.without_identity_mappings(db))
        {
            Some(arguments) => RecursiveMapping::specialize(db, &env, unfolded, arguments),
            None => unfolded,
        }
    }

    /// Close the stored body before transforming it, independently of actual arguments.
    fn unfolded_constructor_body(self, db: &'db dyn Db) -> Type<'db> {
        let env = self.environment(db);
        let mut unfolded = self.body(db).apply_type_mapping_impl(
            db,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Unfold(self.original_constructor(db)),
            )),
            TypeContext::default(),
            &ApplyTypeMappingVisitor::new(&env),
        );
        if let Some(promotion) = self.promotion(db) {
            for (index, (mode, kind)) in promotion.promotions.iter().enumerate() {
                let context = RecursivePromotionContext {
                    source: promotion.source,
                    mode,
                };
                let visitor = ApplyTypeMappingVisitor {
                    recursive_promotion: (index == 0).then_some(&context),
                    ..ApplyTypeMappingVisitor::new(&env).for_recursive_unfolding()
                };
                unfolded = unfolded.apply_type_mapping_impl(
                    db,
                    &TypeMapping::Promote(mode, kind),
                    TypeContext::default(),
                    &visitor,
                );
            }
        }
        unfolded
    }

    /// Transform a recursive constructor's arguments and body without semantic comparisons.
    /// Cycle substitution respects the scope of nested recursive binders.
    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        visitor.with_structural_reconstruction(|| {
            if let TypeMapping::Promote(mode, kind) = mapping
                && let RecursiveOrigin::Lambda(_) = self.origin(db)
            {
                return self.promote(db, *mode, *kind, tcx, visitor);
            }
            match mapping {
                TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                    RecursiveSubstitution::Bind(cycle),
                )) if self.cycle(db) == *cycle => {
                    let arguments = self.arguments(db).map(|arguments| {
                        arguments.apply_type_mapping_impl(db, mapping, &[], visitor)
                    });
                    Type::RecursiveVar(RecursiveVar::new_internal(
                        db,
                        self.cycle(db),
                        arguments,
                        self.materialization_kind(db),
                        self.promotion(db),
                    ))
                }
                TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(substitution)) => {
                    // This binder shadows the target in its body, but not in its arguments.
                    let body = if self.cycle(db) == substitution.cycle(db) {
                        self.body(db)
                    } else {
                        self.body(db)
                            .apply_type_mapping_impl(db, mapping, tcx, visitor)
                    };
                    let arguments = self.arguments(db).map(|arguments| {
                        arguments.apply_type_mapping_impl(db, mapping, &[], visitor)
                    });
                    Type::Recursive(Self::new_internal(
                        db,
                        self.origin(db),
                        self.cycle(db),
                        body,
                        arguments,
                        self.materialization_kind(db),
                        self.promotion(db),
                    ))
                }
                TypeMapping::ApplySpecialization(_)
                | TypeMapping::ApplySpecializationWithMaterialization { .. }
                | TypeMapping::BindLegacyTypevars(_)
                | TypeMapping::FreshenBoundTypeVars { .. }
                | TypeMapping::BindSelf(_)
                | TypeMapping::NormalizeLambdaSignatures
                | TypeMapping::ReplaceSelf { .. } => {
                    // These mappings substitute free variables, which are captured by the constructor's
                    // arguments. Its formal body must remain independent of the calling context.
                    let arguments = self.arguments(db).map(|arguments| {
                        arguments.apply_type_mapping_impl(db, mapping, &[], visitor)
                    });
                    Type::Recursive(self.with_arguments(db, arguments))
                }
                TypeMapping::Materialize(_) if self.materialization_kind(db).is_some() => {
                    Type::Recursive(self)
                }
                TypeMapping::Materialize(kind) => {
                    visitor.visit(db, Type::Recursive(self), mapping, || {
                        self.unfold_with_materialization_visitor(db, visitor)
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
                        self.unfold_with_materialization_visitor(db, visitor)
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
                    let arguments = self.arguments(db).map(|arguments| {
                        arguments.apply_type_mapping_impl(db, mapping, &[], visitor)
                    });
                    let recursive = self.with_arguments(db, arguments);
                    recursive
                        .unfold_with_materialization_visitor(db, visitor)
                        .map(|unfolded| {
                            let mapped =
                                unfolded.apply_type_mapping_impl(db, mapping, tcx, visitor);
                            if mapped == unfolded {
                                Type::Recursive(recursive)
                            } else {
                                mapped
                            }
                        })
                        .into_type()
                }),
            }
        })
    }

    pub(in crate::types) fn variance_equation(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = self.environment(db);
        // Equations describe formal parameters. Applied arguments contribute separate terms,
        // so a reference such as `R[list[T]]` does not unfold a larger body on every visit.
        self.constructor(db)
            .unfold(db, &env)
            .map(|unfolded| unfolded.variance_of(db, &env, typevar))
            .unwrap_or(VarianceTerm::BIVARIANT)
    }
}

#[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
fn promotion_parameters<'db>(
    db: &'db dyn Db,
    source: LambdaSignature<'db>,
) -> Option<GenericContext<'db>> {
    let env = ProgramEnvironment::from_scope(source.scope(db));
    GenericContext::try_from_typevar_instances(
        db,
        &env,
        source
            .captured_typevars(db)
            .into_iter()
            .flat_map(|parameters| parameters.variables(db))
            .flat_map(|variable| {
                RecursivePromotionPosition::ALL
                    .map(move |position| variable.recursive_promotion_slot(db, source, position))
            }),
    )
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

/// A sufficient structural proof of equivalence between finite recursive constructors.
///
/// Each comparison adds only conjunctive obligations over stored types. Recursive applications
/// compare their arguments separately from their formal constructor bodies, so arguments such as
/// `list[T]` never grow through repeated specialization. Unsupported unequal forms are left to
/// the ordinary relation checker.
#[derive(Default)]
pub(super) struct RecursiveConstructorProof<'db> {
    completed: RefCell<FxHashMap<(RecursiveType<'db>, RecursiveType<'db>), bool>>,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum ConstructorObligation<'db> {
    Types(Type<'db>, Type<'db>),
    Constructors(RecursiveType<'db>, RecursiveType<'db>),
}

#[derive(Default)]
struct ConstructorComparison<'db> {
    pending: Vec<ConstructorObligation<'db>>,
    seen: FxHashSet<ConstructorObligation<'db>>,
}

impl<'db> RecursiveConstructorProof<'db> {
    fn equivalent(
        &self,
        db: &'db dyn Db,
        source: RecursiveType<'db>,
        target: RecursiveType<'db>,
    ) -> bool {
        if source == target {
            return true;
        }
        if let Some(result) = self.completed.borrow().get(&(source, target)) {
            return *result;
        }
        let mut comparison = ConstructorComparison::default();
        comparison
            .pending
            .push(ConstructorObligation::Constructors(source, target));
        let result = comparison.prove(db, &source.environment(db));
        self.completed.borrow_mut().insert((source, target), result);
        result
    }
}

impl<'db> ConstructorComparison<'db> {
    fn types(&mut self, source: Type<'db>, target: Type<'db>) {
        if source != target {
            self.pending
                .push(ConstructorObligation::Types(source, target));
        }
    }

    fn type_lists(&mut self, source: &[Type<'db>], target: &[Type<'db>]) -> bool {
        if source.len() != target.len() {
            return false;
        }
        for (&source, &target) in source.iter().zip(target) {
            self.types(source, target);
        }
        true
    }

    fn sets(
        &mut self,
        source: impl IntoIterator<Item = Type<'db>>,
        target: impl IntoIterator<Item = Type<'db>>,
    ) -> bool {
        let source: Vec<_> = source.into_iter().collect();
        let target: Vec<_> = target.into_iter().collect();
        if source.len() != target.len() {
            return false;
        }
        // Exact matches can occur in different positions after rebuilding a set. Pair the
        // remaining elements in stored order; an ambiguous correspondence need not be proved.
        let source_unmatched: Vec<_> = source
            .iter()
            .copied()
            .filter(|ty| !target.contains(ty))
            .collect();
        let target_unmatched: Vec<_> = target
            .iter()
            .copied()
            .filter(|ty| !source.contains(ty))
            .collect();
        self.type_lists(&source_unmatched, &target_unmatched)
    }

    fn tuples(&mut self, source: &TupleSpec<'db>, target: &TupleSpec<'db>) -> bool {
        match (source, target) {
            (Tuple::Fixed(source), Tuple::Fixed(target)) => {
                self.type_lists(source.all_elements(), target.all_elements())
            }
            (Tuple::Variable(source), Tuple::Variable(target)) => {
                if !self.type_lists(source.prefix_elements(), target.prefix_elements())
                    || !self.type_lists(source.suffix_elements(), target.suffix_elements())
                {
                    return false;
                }
                match (source.variable(), target.variable()) {
                    (
                        VariableSegment::Homogeneous(source),
                        VariableSegment::Homogeneous(target),
                    ) => {
                        self.types(source, target);
                        true
                    }
                    (
                        VariableSegment::TypeVarTuple(source),
                        VariableSegment::TypeVarTuple(target),
                    ) => {
                        self.types(Type::TypeVar(source), Type::TypeVar(target));
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn specializations(
        &mut self,
        db: &'db dyn Db,
        source: Option<Specialization<'db>>,
        target: Option<Specialization<'db>>,
    ) -> bool {
        match (source, target) {
            (None, None) => true,
            (Some(source), Some(target)) => {
                source.generic_context(db) == target.generic_context(db)
                    && source.materialization_kind(db) == target.materialization_kind(db)
                    && self.type_lists(source.types(db), target.types(db))
                    && match (source.tuple(db), target.tuple(db)) {
                        (None, None) => true,
                        (Some(source), Some(target)) => self.tuples(source, target),
                        _ => false,
                    }
            }
            _ => false,
        }
    }

    fn recursive_arguments(
        &mut self,
        db: &'db dyn Db,
        source: RecursiveType<'db>,
        target: RecursiveType<'db>,
    ) -> bool {
        if source.materialization_kind(db) != target.materialization_kind(db) {
            return false;
        }
        let (Some(source_arguments), Some(target_arguments)) =
            (source.arguments(db), target.arguments(db))
        else {
            return source.arguments(db) == target.arguments(db);
        };
        if source_arguments.generic_context(db) != target_arguments.generic_context(db)
            || source_arguments.materialization_kind(db)
                != target_arguments.materialization_kind(db)
            || source_arguments.tuple(db) != target_arguments.tuple(db)
            || source_arguments.types(db).len() != target_arguments.types(db).len()
        {
            return false;
        }
        for (parameter, (&source_argument, &target_argument)) in
            source_arguments.generic_context(db).variables(db).zip(
                source_arguments
                    .types(db)
                    .iter()
                    .zip(target_arguments.types(db)),
            )
        {
            if source_argument != target_argument
                && [source, target].into_iter().all(|constructor| {
                    VarianceTerm::variable(
                        db,
                        VarianceOrigin::Recursive(constructor.constructor(db)),
                        parameter.identity(db),
                    )
                    .evaluate(db)
                        == TypeVarVariance::Bivariant
                })
            {
                continue;
            }
            self.types(source_argument, target_argument);
        }
        true
    }

    /// Open only identity applications. A one-sided unfolding must not specialize growing
    /// arguments, and a chain with no observable constructor does not prove an arbitrary type.
    fn open_identity_head(db: &'db dyn Db, mut ty: Type<'db>) -> Option<Type<'db>> {
        let mut seen = FxHashSet::default();
        while let Type::Recursive(recursive) = ty {
            if recursive.arguments(db) != recursive.constructor(db).arguments(db)
                || !seen.insert(recursive)
            {
                return None;
            }
            let UnfoldResult::Unfolded(body) = recursive.unfold(db, &recursive.environment(db))
            else {
                return None;
            };
            ty = body;
        }
        Some(ty)
    }

    fn prove(&mut self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        while let Some(obligation) = self.pending.pop() {
            if !self.seen.insert(obligation) {
                continue;
            }
            let (source, target) = match obligation {
                ConstructorObligation::Constructors(source, target) => {
                    if source == target {
                        continue;
                    }
                    if source.origin(db) != target.origin(db)
                        || source.cycle(db) != target.cycle(db)
                        || source.parameters(db) != target.parameters(db)
                    {
                        return false;
                    }
                    let UnfoldResult::Unfolded(source_body) =
                        source.unfold(db, &source.environment(db))
                    else {
                        return false;
                    };
                    let UnfoldResult::Unfolded(target_body) =
                        target.unfold(db, &target.environment(db))
                    else {
                        return false;
                    };
                    self.types(source_body, target_body);
                    continue;
                }
                ConstructorObligation::Types(source, target) => (source, target),
            };
            match (source, target) {
                (Type::Recursive(source), Type::Recursive(target)) => {
                    if !self.recursive_arguments(db, source, target) {
                        return false;
                    }
                    self.pending.push(ConstructorObligation::Constructors(
                        source.constructor(db),
                        target.constructor(db),
                    ));
                }
                (Type::Recursive(_), _) | (_, Type::Recursive(_)) => {
                    let (Some(source), Some(target)) = (
                        Self::open_identity_head(db, source),
                        Self::open_identity_head(db, target),
                    ) else {
                        return false;
                    };
                    self.types(source, target);
                }
                (Type::NominalInstance(source), Type::NominalInstance(target)) => {
                    match (source.own_tuple_spec(db), target.own_tuple_spec(db)) {
                        (Some(source), Some(target)) => {
                            if !self.tuples(&source, &target) {
                                return false;
                            }
                        }
                        (None, None) => {
                            if source.inherits_from_explicit_any()
                                != target.inherits_from_explicit_any()
                                || source.is_sys_version_info() != target.is_sys_version_info()
                            {
                                return false;
                            }
                            let (source_class, source_args) =
                                source.class(db, env).class_literal_and_specialization(db);
                            let (target_class, target_args) =
                                target.class(db, env).class_literal_and_specialization(db);
                            if source_class != target_class
                                || !self.specializations(db, source_args, target_args)
                            {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                }
                (Type::GenericAlias(source), Type::GenericAlias(target)) => {
                    if source.origin(db) != target.origin(db)
                        || !self.specializations(
                            db,
                            Some(source.specialization(db)),
                            Some(target.specialization(db)),
                        )
                    {
                        return false;
                    }
                }
                (Type::Callable(source), Type::Callable(target)) => {
                    if source.kind(db) != target.kind(db)
                        || source.deprecated(db) != target.deprecated(db)
                    {
                        return false;
                    }
                    let (Some(source), Some(target)) = (
                        source.explicit_signatures(db),
                        target.explicit_signatures(db),
                    ) else {
                        return false;
                    };
                    if source.overloads.len() != target.overloads.len() {
                        return false;
                    }
                    for (source, target) in source.overloads.iter().zip(&target.overloads) {
                        if !source.structurally_matches(target, |source, target| {
                            self.types(source, target);
                        }) {
                            return false;
                        }
                    }
                }
                (Type::Union(source), Type::Union(target)) => {
                    if !self.sets(
                        source.elements(db).iter().copied(),
                        target.elements(db).iter().copied(),
                    ) {
                        return false;
                    }
                }
                (Type::Intersection(source), Type::Intersection(target)) => {
                    if !self.sets(source.iter_positive(db), target.iter_positive(db))
                        || !self.sets(source.iter_negative(db), target.iter_negative(db))
                    {
                        return false;
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Prove a relation between applications of equivalent recursive constructors
    /// based on inclusion or overlap of their type arguments' materialization families.
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
        ) {
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
        let source_constructor = source.constructor(db);
        let target_constructor = target.constructor(db);
        // Argument relations are meaningful only after the finite constructor bodies
        // match. Comparing arguments first can expand unrelated recursive signatures.
        if !self
            .materialization_visitor
            .recursive_constructor_equivalence
            .get_or_init(Box::default)
            .equivalent(db, source_constructor, target_constructor)
        {
            return self.never();
        }
        let argument_pairs = || {
            source_arguments
                .generic_context(db)
                .variables(db)
                .zip(
                    source_arguments
                        .types(db)
                        .iter()
                        .zip(target_arguments.types(db)),
                )
                .filter(|(parameter, (source, target))| {
                    source == target
                        || [source_constructor, target_constructor]
                            .into_iter()
                            .any(|constructor| {
                                VarianceTerm::variable(
                                    db,
                                    VarianceOrigin::Recursive(constructor),
                                    parameter.identity(db),
                                )
                                .evaluate(db)
                                    != TypeVarVariance::Bivariant
                            })
                })
                .map(|(_, pair)| pair)
        };

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
        argument_pairs().when_all(db, self.constraints, |(source, target)| {
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
    unfolded.apply_type_mapping_impl(
        db,
        &TypeMapping::Materialize(kind),
        TypeContext::default(),
        &ApplyTypeMappingVisitor::new(&env).for_recursive_unfolding(),
    )
}

impl<'db> VarianceInferable<'db> for RecursiveType<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let constructor = self.constructor(db);
        let Some(arguments) = self.arguments(db) else {
            return VarianceTerm::variable(db, VarianceOrigin::Recursive(constructor), typevar);
        };
        VarianceTerm::join(
            db,
            arguments
                .generic_context(db)
                .variables(db)
                .zip(arguments.types(db))
                .map(|(parameter, argument)| {
                    VarianceTerm::variable(
                        db,
                        VarianceOrigin::Recursive(constructor),
                        parameter.identity(db),
                    )
                    .compose_thunk(db, || {
                        parameter.argument_variance(db, env, *argument, typevar)
                    })
                }),
        )
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
