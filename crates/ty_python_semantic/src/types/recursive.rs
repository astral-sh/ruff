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

use ruff_python_ast::name::Name;
use rustc_hash::FxHashMap;
use salsa::plumbing::AsId;
use ty_python_core::definition::Definition;
use ty_python_core::place_table;

use super::constraints::{ConstraintSet, IteratorConstraintsExtension};
use super::generics::{ApplySpecialization, Specialization, walk_specialization_types};
use super::relation::{TypeRelation, TypeRelationChecker};
use super::type_alias::AliasCycleSummary;
use super::variance::{VarianceInferable, VarianceOrigin};
use super::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use super::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, GenericAlias,
    GenericContext, KnownClass, MaterializationKind, PromotionKind, PromotionMode, Type,
    TypeAliasType, TypeContext, TypeMapping, TypeVarVariance, VarianceTerm,
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
    /// A transformation of the source binder, before transformations of its enclosing constructor.
    #[returns(copy)]
    operation: Option<RecursiveOperation<'db>>,
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
                Type::Recursive(recursive.instantiate(db, arguments, self.operation(db), visitor))
            }
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Escape(recursive),
            )) if self.cycle(db) == recursive.cycle(db)
                && let Some(context) = visitor.recursive_mapping =>
            {
                context.reference(
                    db,
                    Self::new_internal(db, self.cycle(db), arguments, self.operation(db)),
                    None,
                )
            }
            TypeMapping::ApplyRecursiveSubstitution(_) => Type::RecursiveVar(Self::new_internal(
                db,
                self.cycle(db),
                arguments,
                self.operation(db),
            )),
            _ => unreachable!("semantic operation on an unbound recursive variable"),
        }
    }
}

/// A structural substitution that only the recursive-type binder can construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
pub struct RecursiveMapping<'db>(RecursiveSubstitution<'db>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, get_size2::GetSize)]
enum RecursiveSubstitution<'db> {
    /// Resolve named aliases into closed structural constructors before semantic mapping.
    ResolveAliases,
    /// Replace references to a binder with applications of its recursive constructor.
    /// In the module example, this replaces `F[list[T]]` with `Tree[list[T]]`.
    Unfold(RecursiveType<'db>),
    /// Replace applications of the target binder with variables to form an open body.
    /// In the module example, this replaces `Tree[list[T]]` with `F[list[T]]`.
    Bind(RecursiveType<'db>),
    Escape(RecursiveType<'db>),
    Restore(RecursiveCycle),
}

impl RecursiveSubstitution<'_> {
    fn cycle(self, db: &dyn Db) -> Option<RecursiveCycle> {
        match self {
            Self::ResolveAliases => None,
            Self::Unfold(recursive) | Self::Bind(recursive) | Self::Escape(recursive) => {
                Some(recursive.cycle(db))
            }
            Self::Restore(cycle) => Some(cycle),
        }
    }
}

impl<'db> RecursiveMapping<'db> {
    /// Resolve alias constructors or substitute only their stored arguments.
    pub(super) fn apply_alias(
        self,
        db: &'db dyn Db,
        alias: TypeAliasType<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        match self.0 {
            RecursiveSubstitution::ResolveAliases => alias.structural_type(db, visitor),
            RecursiveSubstitution::Bind(recursive)
                if alias.definition(db) == recursive.definition(db) =>
            {
                let arguments = alias
                    .specialization(db)
                    .or_else(|| {
                        alias
                            .generic_context(db)
                            .map(|parameters| parameters.default_specialization(db, None))
                    })
                    .map(|arguments| {
                        arguments.apply_type_mapping_impl(
                            db,
                            &TypeMapping::ApplyRecursiveSubstitution(self),
                            &[],
                            visitor,
                        )
                    });
                Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    recursive.cycle(db),
                    arguments,
                    None,
                ))
            }
            _ => Type::TypeAlias(alias.map_stored_specialization(
                db,
                &TypeMapping::ApplyRecursiveSubstitution(self),
                visitor,
            )),
        }
    }
}

#[salsa::tracked]
impl<'db> TypeAliasType<'db> {
    /// Bind named references before transformations can expand them into successive approximations.
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, alias: TypeAliasType<'db>, _| Type::TypeAlias(alias),
        cycle_fn=|db, cycle: &salsa::Cycle, _, result, alias: TypeAliasType<'db>, parameters| {
            RecursiveType::recover(db, alias.definition(db), cycle.id(), parameters, result)
        },
        heap_size=ruff_memory_usage::heap_size
    )]
    fn recursive_constructor(
        self,
        db: &'db dyn Db,
        parameters: Option<GenericContext<'db>>,
    ) -> Type<'db> {
        // The caller resolves parameters before entering the cycle; only recovery uses them.
        let _ = parameters;
        let env = ProgramEnvironment::from_definition(self.definition(db));
        self.raw_value_type(db).apply_type_mapping(
            db,
            &env,
            &TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::ResolveAliases,
            )),
            TypeContext::default(),
        )
    }

    /// Close alias references before mapping, sharing one constructor across specializations.
    pub(super) fn structural_type(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let resolve = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
            RecursiveSubstitution::ResolveAliases,
        ));
        let parameters = self.generic_context(db);
        let constructor = self.unspecialized(db).recursive_constructor(db, parameters);
        // Helpers remain opaque so specialization cannot normalize recursive arguments
        // before binding. Members of a mutual cycle must expose their constructor bodies.
        if !self.is_recursive(db) {
            return Type::TypeAlias(self.map_stored_specialization(db, &resolve, visitor));
        }
        let mut ty = constructor;
        if let Some(arguments) = self
            .specialization(db)
            .or_else(|| parameters.map(|parameters| parameters.default_specialization(db, None)))
        {
            let arguments = arguments.apply_type_mapping_impl(db, &resolve, &[], visitor);
            ty = ty.apply_type_mapping_impl(
                db,
                &TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(arguments)),
                TypeContext::default(),
                visitor,
            );
        }
        ty
    }
}

/// Identifies an alias query and names its recursive binder and variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecursiveCycle(salsa::Id);

impl get_size2::GetSize for RecursiveCycle {}

/// An application of a structural recursive type constructor.
/// An original constructor's private body remains unspecialized. A mapped constructor
/// stores a specialized body and retains its origin to reconstruct it when a recursive
/// occurrence supplies different arguments. Opposite-polarity states are nested binders
/// within that body.
/// Unfolding replaces recursive references with closed types before exposing the result
/// to ordinary type operations.
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
    /// Unfolding applies them to an original body; mapped bodies already specialize them.
    #[returns(copy)]
    pub(super) arguments: Option<Specialization<'db>>,
    /// The constructor before mapping, used when a recursive occurrence supplies new arguments.
    #[returns(copy)]
    origin: Option<RecursiveMappingOrigin<'db>>,
}

impl get_size2::GetSize for RecursiveType<'_> {}

/// A closed placeholder for one recursive occurrence while its body is transformed.
#[salsa::interned(debug)]
struct EscapedRecursiveVar<'db> {
    #[returns(copy)]
    source: RecursiveType<'db>,
    #[returns(copy)]
    variable: RecursiveVar<'db>,
    #[returns(copy)]
    negative: Option<bool>,
}

/// Names a transformed binder independently of provisional bodies and type arguments.
#[salsa::interned(debug)]
struct MappedRecursiveCycle<'db> {
    source: RecursiveCycle,
    operation: RecursiveOperation<'db>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
/// An operation that can be reapplied when a recursive occurrence supplies new arguments.
pub enum RecursiveOperation<'db> {
    Promote(PromotionMode, PromotionKind),
    Materialize(MaterializationKind),
    RescopeReturnCallables(GenericContext<'db>),
}

impl<'db> RecursiveOperation<'db> {
    fn mapping(self) -> TypeMapping<'db, 'db> {
        match self {
            Self::Promote(mode, kind) => TypeMapping::Promote(mode, kind),
            Self::Materialize(kind) => TypeMapping::Materialize(kind),
            Self::RescopeReturnCallables(context) => TypeMapping::RescopeReturnCallables(context),
        }
    }

    fn flip(self) -> Self {
        match self {
            Self::Promote(mode, kind) => Self::Promote(mode.flip(), kind),
            Self::Materialize(kind) => Self::Materialize(kind.flip()),
            Self::RescopeReturnCallables(_) => self,
        }
    }
}

/// Retains the source constructor needed to transform later recursive applications.
#[salsa::interned(debug)]
pub struct RecursiveMappingOrigin<'db> {
    /// Binding can temporarily replace the constructor with a reference to an enclosing binder.
    #[returns(copy)]
    source: Type<'db>,
    #[returns(copy)]
    operation: RecursiveOperation<'db>,
}

impl get_size2::GetSize for RecursiveMappingOrigin<'_> {}

impl<'db> RecursiveMappingOrigin<'db> {
    /// Access the source after references to enclosing binders have been substituted.
    fn constructor(self, db: &'db dyn Db) -> RecursiveType<'db> {
        match self.source(db) {
            Type::Recursive(source) => source,
            _ => unreachable!("semantic operation on an unbound recursive mapping source"),
        }
    }
}

/// Nests newly reached mapping states and binds references to states already being constructed.
/// For bodies `P(a, b)` and `N(a, b)`, this constructs `μa. P(a, μb. N(a, b))`.
pub(super) struct RecursiveTypeMapping<'a, 'db> {
    parent: Option<&'a RecursiveTypeMapping<'a, 'db>>,
    source: RecursiveType<'db>,
    operation: RecursiveOperation<'db>,
    references: RefCell<FxHashMap<Type<'db>, EscapedRecursiveVar<'db>>>,
}

impl<'db> RecursiveTypeMapping<'_, 'db> {
    fn polarity(mapping: &TypeMapping<'_, 'db>) -> bool {
        matches!(
            mapping,
            TypeMapping::Promote(PromotionMode::Off, _)
                | TypeMapping::Materialize(MaterializationKind::Bottom)
        )
    }

    fn reference(
        &self,
        db: &'db dyn Db,
        variable: RecursiveVar<'db>,
        negative: Option<bool>,
    ) -> Type<'db> {
        let reference = EscapedRecursiveVar::new(db, self.source, variable, negative);
        let fresh = Type::TypeVar(BoundTypeVarInstance::synthetic(
            db,
            &self.source.environment(db),
            Name::new(format!("$recursive:{:?}", reference.as_id())),
            TypeVarVariance::Invariant,
        ));
        self.references.borrow_mut().insert(fresh, reference);
        fresh
    }

    /// Recover a closed occurrence when discovering the variables a callable must bind.
    pub(super) fn original_reference(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Type<'db>> {
        let Some(reference) = self.references.borrow().get(&ty).copied() else {
            return self.parent?.original_reference(db, ty, visitor);
        };
        let variable = reference.variable(db);
        Some(Type::Recursive(self.source.instantiate(
            db,
            variable.arguments(db),
            variable.operation(db),
            visitor,
        )))
    }

    fn cycle(&self, db: &'db dyn Db) -> RecursiveCycle {
        RecursiveCycle(MappedRecursiveCycle::new(db, self.source.cycle(db), self.operation).as_id())
    }

    /// Nest a new mapping state, or close a reference to a state already being constructed.
    fn restore_reference(
        &self,
        db: &'db dyn Db,
        mut variable: RecursiveVar<'db>,
        mut operation: RecursiveOperation<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if self.source.origin(db).is_none()
            && matches!(operation, RecursiveOperation::Materialize(_))
            && let Some(previous @ RecursiveOperation::Materialize(_)) = variable.operation(db)
        {
            // A materialized reference is static, regardless of its position's polarity.
            operation = previous;
            variable =
                RecursiveVar::new_internal(db, variable.cycle(db), variable.arguments(db), None);
        }
        let mut context = Some(self);
        while let Some(active) = context {
            if active.source.cycle(db) == self.source.cycle(db) && active.operation == operation {
                return Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    active.cycle(db),
                    variable.arguments(db),
                    variable.operation(db),
                ));
            }
            context = active.parent;
        }
        let source =
            self.source
                .instantiate(db, variable.arguments(db), variable.operation(db), visitor);
        Type::Recursive(Self::apply(db, source, operation, visitor))
    }

    pub(super) fn map_reference(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Type<'db>> {
        let Some(reference) = self.references.borrow().get(&ty).copied() else {
            return self.parent?.map_reference(db, ty, mapping, visitor);
        };
        let variable = reference.variable(db);
        if let TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
            RecursiveSubstitution::Restore(target),
        )) = mapping
        {
            // Nested mappings leave references owned by an enclosing binder escaped.
            if *target != self.cycle(db) {
                return None;
            }
            if reference.negative(db).is_none()
                && self.source.origin(db).is_none()
                && let Some(operation) = variable.operation(db)
                && (operation == self.operation || operation == self.operation.flip())
            {
                // This state is being constructed; refer to its binder instead of rebuilding it.
                let variable = RecursiveVar::new_internal(
                    db,
                    variable.cycle(db),
                    variable.arguments(db),
                    None,
                );
                return Some(self.restore_reference(db, variable, operation, visitor));
            }
            return Some(match reference.negative(db) {
                Some(negative) => self.restore_reference(
                    db,
                    variable,
                    if negative {
                        self.operation.flip()
                    } else {
                        self.operation
                    },
                    visitor,
                ),
                None => Type::Recursive(self.source.instantiate(
                    db,
                    variable.arguments(db),
                    variable.operation(db),
                    visitor,
                )),
            });
        }
        if mapping.is_structural() {
            return None;
        }
        if matches!(
            mapping,
            TypeMapping::ApplySpecialization(_)
                | TypeMapping::ApplySpecializationWithMaterialization { .. }
        ) {
            let arguments = variable
                .arguments(db)
                .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
            let variable = RecursiveVar::new_internal(
                db,
                variable.cycle(db),
                arguments,
                variable.operation(db),
            );
            return Some(self.reference(db, variable, reference.negative(db)));
        }
        let negative = Self::polarity(mapping) != Self::polarity(&self.operation.mapping());
        Some(self.reference(db, variable, Some(negative)))
    }

    fn apply(
        db: &'db dyn Db,
        source: RecursiveType<'db>,
        operation: RecursiveOperation<'db>,
        outer: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> RecursiveType<'db> {
        if matches!(operation, RecursiveOperation::Materialize(_))
            && (source.materialization_kind(db).is_some()
                || source.mapping_is_noop(db, operation, outer))
        {
            return source;
        }
        if let Some(origin) = source.origin(db)
            && origin.operation(db) == operation
        {
            return source;
        }
        let context = RecursiveTypeMapping {
            parent: outer.recursive_mapping,
            source,
            operation,
            references: RefCell::default(),
        };
        let visitor = ApplyTypeMappingVisitor {
            recursive_mapping: Some(&context),
            ..outer.for_new_materialization_root()
        };
        let escape = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
            RecursiveSubstitution::Escape(source),
        ));
        let restore = TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
            RecursiveSubstitution::Restore(context.cycle(db)),
        ));
        let mut body =
            source
                .body(db)
                .apply_type_mapping_impl(db, &escape, TypeContext::default(), &visitor);
        if source.origin(db).is_none()
            && let Some(arguments) = source.arguments(db)
        {
            let specialization = match arguments.materialization_kind(db) {
                None => TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(arguments)),
                Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                    specialization: ApplySpecialization::TypeAlias(arguments),
                    materialization_kind,
                },
            };
            let specialization_visitor = ApplyTypeMappingVisitor {
                recursive_mapping: Some(&context),
                ..outer.for_new_materialization_root()
            };
            body = body.apply_type_mapping_impl(
                db,
                &specialization,
                TypeContext::default(),
                &specialization_visitor,
            );
        }
        // Each stage needs fresh mapping caches, but nested mappings share comparison guards.
        let mapping_visitor = ApplyTypeMappingVisitor {
            recursive_mapping: Some(&context),
            ..outer.for_new_materialization_root()
        };
        let mapped = body.apply_type_mapping_impl(
            db,
            &operation.mapping(),
            TypeContext::default(),
            &mapping_visitor,
        );
        if mapped == body {
            return source;
        }
        let restore_visitor = ApplyTypeMappingVisitor {
            recursive_mapping: Some(&context),
            ..outer.for_new_materialization_root()
        };
        let body =
            mapped.apply_type_mapping_impl(db, &restore, TypeContext::default(), &restore_visitor);
        let arguments = source.arguments(db);
        let origin = RecursiveMappingOrigin::new(db, Type::Recursive(source), operation);
        RecursiveType::new_internal(
            db,
            source.definition(db),
            context.cycle(db),
            body,
            arguments,
            Some(origin),
        )
    }
}

#[salsa::tracked]
impl<'db> RecursiveType<'db> {
    /// The materialization producing this body, if it is the outermost operation.
    fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        let RecursiveOperation::Materialize(kind) = self.origin(db)?.operation(db) else {
            return None;
        };
        Some(kind)
    }

    /// Prove that both polarities preserve this instantiation and its recursive unfoldings.
    /// Promotion treats free variables as opaque; later specializing them requires a new proof.
    fn mapping_is_noop(
        self,
        db: &'db dyn Db,
        operation: RecursiveOperation<'db>,
        outer: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> bool {
        struct UnchangedConstructorVisitor<'a, 'db> {
            outer: &'a ApplyTypeMappingVisitor<'a, 'db>,
            operation: RecursiveOperation<'db>,
            recursion_guard: TypeCollector<'db>,
            parameters: RefCell<Vec<GenericContext<'db>>>,
            unchanged: Cell<bool>,
        }

        impl<'db> UnchangedConstructorVisitor<'_, 'db> {
            fn visit_body(
                &self,
                db: &'db dyn Db,
                parameters: Option<GenericContext<'db>>,
                body: Type<'db>,
            ) {
                if let Some(parameters) = parameters {
                    self.parameters.borrow_mut().push(parameters);
                }
                self.visit_type(db, body);
                if parameters.is_some() {
                    self.parameters.borrow_mut().pop();
                }
            }
        }

        impl<'db> TypeVisitor<'db> for UnchangedConstructorVisitor<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.outer.env
            }

            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn notify_skipped_lazy_type_attributes(&self) {
                self.unchanged.set(false);
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                if !self.unchanged.get() {
                    return;
                }
                let mut context = self.outer.recursive_mapping;
                while let Some(mapping) = context {
                    if mapping.references.borrow().contains_key(&ty) {
                        // The enclosing binder's escaped reference has not been proved unchanged.
                        self.unchanged.set(false);
                        return;
                    }
                    context = mapping.parent;
                }
                // Singleton promotion stops at non-nominal types. Generic aliases are visited
                // here through a nominal instance's class specialization.
                if matches!(
                    self.operation,
                    RecursiveOperation::Promote(_, PromotionKind::SingletonsOnly)
                ) && !matches!(
                    ty,
                    Type::NominalInstance(_)
                        | Type::GenericAlias(_)
                        | Type::Recursive(_)
                        | Type::RecursiveVar(_)
                ) {
                    return;
                }
                match ty {
                    Type::TypeVar(variable) => {
                        // Promotion preserves an unspecialized variable in a comparison. The
                        // stored mapping still applies if it is later specialized to a literal.
                        // Materialization additionally transforms variable bounds and defaults.
                        self.unchanged.set(
                            matches!(self.operation, RecursiveOperation::Promote(..))
                                || self.parameters.borrow().iter().any(|parameters| {
                                    parameters.contains(db, variable.identity(db))
                                }),
                        );
                    }
                    Type::RecursiveVar(variable) => {
                        if variable.operation(db).is_some() {
                            self.unchanged.set(false);
                        } else if let Some(arguments) = variable.arguments(db) {
                            walk_specialization_types(db, arguments, self);
                        }
                    }
                    Type::Dynamic(_) => {
                        self.unchanged
                            .set(matches!(self.operation, RecursiveOperation::Promote(..)));
                    }
                    Type::ClassLiteral(_)
                        if matches!(
                            self.operation,
                            RecursiveOperation::Promote(_, PromotionKind::ClassLiteralsOnly)
                        ) =>
                    {
                        self.unchanged.set(false);
                    }
                    Type::LiteralValue(_)
                        if matches!(
                            self.operation,
                            RecursiveOperation::Promote(_, PromotionKind::Regular)
                        ) =>
                    {
                        self.unchanged.set(false);
                    }
                    Type::NominalInstance(instance)
                        if match self.operation {
                            RecursiveOperation::Promote(_, PromotionKind::Regular) => matches!(
                                instance.known_class(db),
                                Some(KnownClass::Float | KnownClass::Complex)
                            ),
                            RecursiveOperation::Promote(_, PromotionKind::SingletonsOnly) => {
                                instance.is_singleton(db)
                            }
                            _ => false,
                        } =>
                    {
                        self.unchanged.set(false);
                    }
                    Type::Recursive(_)
                    | Type::TypeAlias(_)
                    | Type::Never
                    | Type::ClassLiteral(_)
                    | Type::LiteralValue(_)
                    | Type::NominalInstance(_)
                    | Type::GenericAlias(_)
                    | Type::Callable(_)
                    | Type::Union(_)
                    | Type::Intersection(_) => {
                        walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
                    }
                    _ => self.unchanged.set(false),
                }
            }

            fn visit_recursive_type(&self, db: &'db dyn Db, recursive: RecursiveType<'db>) {
                if matches!(self.operation, RecursiveOperation::Materialize(_))
                    && recursive.materialization_kind(db).is_some()
                {
                    return;
                }
                if let Some(origin) = recursive.origin(db) {
                    // Materialization replaces dynamic leaves with object/Never, which are not
                    // promoted. Singleton promotion differs: a union can expose a singleton.
                    if matches!(
                        self.operation,
                        RecursiveOperation::Promote(
                            _,
                            PromotionKind::Regular | PromotionKind::ClassLiteralsOnly
                        )
                    ) && matches!(origin.operation(db), RecursiveOperation::Materialize(_))
                    {
                        self.visit_type(db, origin.source(db));
                    } else {
                        self.unchanged.set(false);
                    }
                    return;
                }
                if let Some(arguments) = recursive.arguments(db) {
                    walk_specialization_types(db, arguments, self);
                }
                // Inspect the parameterized body and edge arguments, not successive unfoldings:
                // R[list[T]] preserves the assumption, but R[tuple[T, Any]] may not.
                self.visit_body(db, recursive.parameters(db), recursive.body(db));
            }

            fn visit_type_alias_type(&self, db: &'db dyn Db, alias: TypeAliasType<'db>) {
                if let Some(arguments) = alias.specialization(db).or_else(|| {
                    alias
                        .generic_context(db)
                        .map(|parameters| parameters.default_specialization(db, None))
                }) {
                    walk_specialization_types(db, arguments, self);
                }
                self.visit_body(db, alias.generic_context(db), alias.raw_value_type(db));
            }

            fn visit_generic_alias_type(&self, db: &'db dyn Db, alias: GenericAlias<'db>) {
                walk_specialization_types(db, alias.specialization(db), self);
            }
        }

        let visitor = UnchangedConstructorVisitor {
            outer,
            operation,
            recursion_guard: TypeCollector::default(),
            parameters: RefCell::default(),
            unchanged: Cell::new(true),
        };
        visitor.visit_type(db, Type::Recursive(self));
        visitor.unchanged.get()
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
            definition,
            cycle,
            Type::RecursiveVar(RecursiveVar::new_internal(db, cycle, arguments, None)),
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
                RecursiveSubstitution::Bind(self),
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

    fn with_arguments(
        self,
        db: &'db dyn Db,
        arguments: Option<Specialization<'db>>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        self.instantiate(db, arguments, None, visitor)
    }

    /// Instantiate a source reference before replaying transformations of the enclosing constructor.
    fn instantiate(
        self,
        db: &'db dyn Db,
        arguments: Option<Specialization<'db>>,
        operation: Option<RecursiveOperation<'db>>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        if arguments == self.arguments(db) && operation.is_none() {
            return self;
        }
        if let Some(origin) = self.origin(db) {
            let source = origin
                .constructor(db)
                .instantiate(db, arguments, operation, visitor);
            return RecursiveTypeMapping::apply(db, source, origin.operation(db), visitor);
        }
        let recursive = Self::new_internal(
            db,
            self.definition(db),
            self.cycle(db),
            self.body(db),
            arguments,
            self.origin(db),
        );
        match operation {
            Some(operation) => RecursiveTypeMapping::apply(db, recursive, operation, visitor),
            None => recursive,
        }
    }

    /// Recover the input to the outer materialization, retaining earlier transformations.
    fn before_materialization(self, db: &'db dyn Db) -> Self {
        if let Some(origin) = self.origin(db)
            && matches!(origin.operation(db), RecursiveOperation::Materialize(_))
        {
            origin.constructor(db)
        } else {
            self
        }
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

    /// Whether the alias name and materialization still describe this constructor's body.
    pub(super) fn is_alias_application(self, db: &'db dyn Db) -> bool {
        self.origin(db).is_none_or(|origin| {
            matches!(origin.operation(db), RecursiveOperation::Materialize(_))
                && origin.constructor(db).is_alias_application(db)
        })
    }

    /// The source and operation producing this body, without unfolding recursion.
    pub(super) fn mapping_origin(self, db: &'db dyn Db) -> Option<(Self, RecursiveOperation<'db>)> {
        let origin = self.origin(db)?;
        Some((origin.constructor(db), origin.operation(db)))
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
        if let Some(origin) = self.origin(db) {
            return origin.constructor(db).constructor(db);
        }
        let env = self.environment(db);
        self.with_arguments(
            db,
            self.parameters(db)
                .map(|parameters| parameters.identity_specialization(db)),
            &ApplyTypeMappingVisitor::new(&env),
        )
    }

    /// The program in which the recursive type's body was constructed.
    fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        ProgramEnvironment::from_definition(self.definition(db))
    }

    /// Substitute closed types for references before exposing the body.
    ///
    /// Report whether unfolding returns exactly `Type::Recursive(self)`. An unfolded
    /// type can still contain recursive references, so callers must retain their recursion guards.
    pub fn unfold(self, db: &'db dyn Db) -> UnfoldResult<'db> {
        let unfolded = self.unfolded_body(db);
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
        if self.origin(db).is_some() {
            return unfolded;
        }
        match self.arguments(db) {
            Some(arguments) => {
                // Materialize substituted arguments at their occurrences, preserving variance.
                let mapping = match arguments.materialization_kind(db) {
                    None => {
                        TypeMapping::ApplySpecialization(ApplySpecialization::TypeAlias(arguments))
                    }
                    Some(materialization_kind) => {
                        TypeMapping::ApplySpecializationWithMaterialization {
                            specialization: ApplySpecialization::TypeAlias(arguments),
                            materialization_kind,
                        }
                    }
                };
                unfolded.apply_type_mapping(db, &env, &mapping, TypeContext::default())
            }
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
                RecursiveSubstitution::Bind(binding),
            )) if let Some(origin) = self.origin(db)
                && let Type::Recursive(source) = origin.source(db)
                && source.cycle(db) == binding.cycle(db) =>
            {
                // A mapped provisional alias still refers to the source query's eventual body.
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    binding.cycle(db),
                    arguments,
                    Some(origin.operation(db)),
                ))
            }
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(
                RecursiveSubstitution::Bind(binding),
            )) if self.cycle(db) == binding.cycle(db)
                && self.materialization_kind(db).is_none() =>
            {
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                Type::RecursiveVar(RecursiveVar::new_internal(
                    db,
                    self.cycle(db),
                    arguments,
                    None,
                ))
            }
            TypeMapping::ApplyRecursiveSubstitution(RecursiveMapping(substitution)) => {
                // This binder shadows the target in its body, but not in its arguments.
                let body = if Some(self.cycle(db)) == substitution.cycle(db) {
                    self.body(db)
                } else {
                    self.body(db)
                        .apply_type_mapping_impl(db, mapping, tcx, visitor)
                };
                let arguments = self
                    .arguments(db)
                    .map(|arguments| arguments.apply_type_mapping_impl(db, mapping, &[], visitor));
                let origin = self.origin(db).map(|origin| {
                    RecursiveMappingOrigin::new(
                        db,
                        origin
                            .source(db)
                            .apply_type_mapping_impl(db, mapping, tcx, visitor),
                        origin.operation(db),
                    )
                });
                Type::Recursive(Self::new_internal(
                    db,
                    self.definition(db),
                    self.cycle(db),
                    body,
                    arguments,
                    origin,
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
                Type::Recursive(self.with_arguments(db, arguments, visitor))
            }
            TypeMapping::RescopeReturnCallables(context) => {
                Type::Recursive(RecursiveTypeMapping::apply(
                    db,
                    self,
                    RecursiveOperation::RescopeReturnCallables(*context),
                    visitor,
                ))
            }
            TypeMapping::Promote(mode, kind) => Type::Recursive(RecursiveTypeMapping::apply(
                db,
                self,
                RecursiveOperation::Promote(*mode, *kind),
                visitor,
            )),
            TypeMapping::Materialize(kind) => Type::Recursive(RecursiveTypeMapping::apply(
                db,
                self,
                RecursiveOperation::Materialize(*kind),
                visitor,
            )),
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
            TypeMapping::ReplaceParameterDefaults => {
                visitor.visit(db, Type::Recursive(self), mapping, || {
                    // Map arguments before unfolding so recursive backedges retain their mapped
                    // arguments. Keep the application's materialization throughout the traversal.
                    let arguments = self.arguments(db).map(|arguments| {
                        arguments.apply_type_mapping_impl(db, mapping, &[], visitor)
                    });
                    let recursive = self.with_arguments(db, arguments, visitor);
                    recursive
                        .unfold(db)
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
                })
            }
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
    /// Remove a mapped constructor only after proving its operation preserves all unfoldings.
    pub(super) fn without_noop_recursive_mapping(
        &self,
        db: &'db dyn Db,
        mut ty: Type<'db>,
    ) -> Type<'db> {
        while let Type::Recursive(recursive) = ty {
            let Some(origin) = recursive.origin(db) else {
                break;
            };
            let Type::Recursive(source) =
                self.without_noop_recursive_mapping(db, Type::Recursive(origin.constructor(db)))
            else {
                break;
            };
            if !source.mapping_is_noop(db, origin.operation(db), self.materialization_visitor) {
                break;
            }
            ty = Type::Recursive(source);
        }
        ty
    }

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
        let source_base = source.before_materialization(db);
        let target_base = target.before_materialization(db);
        let parameters = Some(
            source_arguments
                .generic_context(db)
                .identity_specialization(db),
        );
        // Materializations share a family only when their pre-materialization bodies agree.
        // The declaration alone is insufficient: promotion can change that body.
        if source_base.with_arguments(db, parameters, self.materialization_visitor)
            != target_base.with_arguments(db, parameters, self.materialization_visitor)
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
        let unmaterialized_target = Type::Recursive(target_base);
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
