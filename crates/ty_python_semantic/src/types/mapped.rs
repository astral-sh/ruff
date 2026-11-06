//! Deferred transformations of recursive types.
//!
//! `Mapped[M, R]` denotes `M(R)`. Exposing it first unfolds `R`, including its
//! specialization, and then applies `M` to that closed body. Recursive children
//! retain their own deferred mappings, including any change of polarity.
//! Transforming the constructor's arguments separately is not equivalent: for
//! `R[T] = tuple[T, R[list[T]]]`, promotion depends on each argument's depth.

use std::cell::{Cell, RefCell};

use rustc_hash::FxHashMap;
use salsa::plumbing::AsId;
use ty_python_core::scope::ScopeId;

use super::cyclic::{ActiveRecursionDetector, TypeIdentity};
use super::generics::{ApplySpecialization, Specialization, walk_specialization_types};
use super::variance::{VarianceInferable, VarianceOrigin, VarianceTerm};
use super::visitor::{TypeVisitor, any_over_type_including_alias_arguments};
use super::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance,
    CallableType, GenericContext, MaterializationKind, PromotionKind, PromotionMode, SelfBinding,
    SelfTypeVarOrigin, Type, TypeContext, TypeMapping, TypeRecursionContext, TypeVarVariance,
    UnfoldResult,
};
use crate::{Db, FxIndexMap, FxOrderSet, Program, ProgramEnvironment};

/// A transformation retained until the outer structure of its operand is needed.
/// The operand is a recursive type, an alias, or another deferred transformation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub struct MappedType<'db>(MappedTypeData<'db>);

#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
struct MappedTypeData<'db> {
    #[returns(copy)]
    program: Program<'db>,
    #[returns(copy)]
    inner: Type<'db>,
    #[returns(ref)]
    mapping: DeferredTypeMapping<'db>,
    #[returns(copy)]
    context: TypeContext<'db>,
}

impl get_size2::GetSize for MappedTypeData<'_> {}

#[salsa::tracked]
impl<'db> MappedTypeData<'db> {
    #[salsa::tracked(
        returns(copy),
        cycle_initial=|_, _, mapped: MappedTypeData<'db>, _| Type::Mapped(MappedType(mapped)),
        heap_size=ruff_memory_usage::heap_size
    )]
    fn exposed(self, db: &'db dyn Db, program: Program<'db>) -> Type<'db> {
        MappedType(self).expose_impl(
            db,
            &ApplyTypeMappingVisitor::new(&ProgramEnvironment::from_program(program)),
        )
    }

    #[salsa::tracked(returns(ref), cycle_initial=|_, _, _| Box::default(), heap_size=ruff_memory_usage::heap_size)]
    fn argument_matches(self, db: &'db dyn Db) -> Box<[bool]> {
        MappedType(self).argument_matches(db)
    }
}

impl<'db> MappedType<'db> {
    fn new_internal(
        db: &'db dyn Db,
        program: Program<'db>,
        inner: Type<'db>,
        mapping: DeferredTypeMapping<'db>,
        context: TypeContext<'db>,
    ) -> Self {
        Self(MappedTypeData::new(db, program, inner, mapping, context))
    }

    pub(super) fn inner(self, db: &'db dyn Db) -> Type<'db> {
        self.0.inner(db)
    }

    fn program(self, db: &'db dyn Db) -> Program<'db> {
        self.0.program(db)
    }

    pub(super) fn mapping(self, db: &'db dyn Db) -> &'db DeferredTypeMapping<'db> {
        self.0.mapping(db)
    }

    fn context(self, db: &'db dyn Db) -> TypeContext<'db> {
        self.0.context(db)
    }

    /// Retain a semantic mapping unless it leaves the recursive component unchanged.
    pub(super) fn defer(
        db: &'db dyn Db,
        inner: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Type<'db>> {
        let deferred = DeferredTypeMapping::capture(mapping, visitor)?;
        if let Some(effects) = visitor.mapping_effects {
            effects.observe(db, inner, mapping, context, visitor);
            return Some(inner);
        }
        if let Type::Mapped(previous) = inner {
            // A materialization is already static. Other transformations can
            // introduce dynamic types again, so only adjacent materializations collapse.
            if matches!(deferred, DeferredTypeMapping::Materialize { typevar_bounds_and_defaults, .. }
                if !typevar_bounds_and_defaults || matches!(previous.mapping(db), DeferredTypeMapping::Materialize { typevar_bounds_and_defaults: true, .. }))
                && previous.materialization_kind(db).is_some()
            {
                return Some(inner);
            }
            if previous.mapping(db) == &deferred
                && previous.context(db) == context
                && matches!(
                    deferred,
                    DeferredTypeMapping::Promote(..)
                        | DeferredTypeMapping::ReplaceParameterDefaults
                        | DeferredTypeMapping::GeneralizeLocalTypes(_)
                )
            {
                return Some(inner);
            }
        }
        let effects = MappingEffects::default();
        effects.observe(
            db,
            inner,
            mapping,
            context,
            &visitor.for_mapping_probe(&effects),
        );
        Some(if effects.changed.get() {
            Type::Mapped(Self::new_internal(
                db,
                visitor.env.program(db),
                inner,
                deferred,
                context,
            ))
        } else {
            inner
        })
    }

    pub(super) fn materialization_kind(self, db: &'db dyn Db) -> Option<MaterializationKind> {
        match self.mapping(db) {
            DeferredTypeMapping::Materialize { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    pub(super) fn is_fully_materialized(self, db: &'db dyn Db) -> bool {
        matches!(
            self.mapping(db),
            DeferredTypeMapping::Materialize {
                typevar_bounds_and_defaults: true,
                ..
            }
        )
    }

    /// The environment in which the deferred transformation was created.
    pub fn environment(self, db: &'db dyn Db) -> ProgramEnvironment<'db> {
        ProgramEnvironment::from_program(self.program(db))
    }

    /// Expose one mapped layer, leaving transformations of recursive children deferred.
    pub fn expose(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> UnfoldResult<'db> {
        self.expose_with_recursion(db, env, None)
    }

    pub(super) fn expose_with_recursion(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        context: Option<&TypeRecursionContext<'db>>,
    ) -> UnfoldResult<'db> {
        self.expose_with_visitor(
            db,
            &ApplyTypeMappingVisitor::new(env).with_recursion_context(context),
        )
    }

    /// Preserve active equivalence checks while materializing a recursive layer.
    /// Restarting those checks would repeatedly compare growing specializations.
    pub(super) fn expose_with_visitor(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> UnfoldResult<'db> {
        let exposed = if visitor.mapping_effects.is_some()
            || visitor
                .recursion_context
                .is_some_and(|context| context.meta_type.is_active())
            || !visitor.materialization_equivalence().is_empty()
        {
            self.expose_impl(db, visitor)
        } else {
            // Composition repeatedly exposes the same inner transformations.
            // Cache their results when no active recursion state can affect them.
            self.0.exposed(db, visitor.env.program(db))
        };
        if exposed == Type::Mapped(self) {
            UnfoldResult::Unchanged(exposed)
        } else {
            UnfoldResult::Unfolded(exposed)
        }
    }

    fn expose_impl(self, db: &'db dyn Db, visitor: &ApplyTypeMappingVisitor<'_, 'db>) -> Type<'db> {
        let inner = match self.inner(db) {
            Type::TypeAlias(alias) => UnfoldResult::Unfolded(
                alias.value_type_with_recursion(db, visitor.recursion_context),
            ),
            Type::Mapped(mapped) => mapped.expose_with_visitor(db, visitor),
            inner => inner.expose(db, visitor.env),
        };
        inner
            .map(|inner| self.mapping(db).apply(db, inner, self.context(db), visitor))
            .unwrap_or(Type::Mapped(self))
    }

    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        if mapping.is_structural() {
            // Rebind graph references in every stored operand without exposing an open body.
            return Type::Mapped(Self::new_internal(
                db,
                self.program(db),
                self.inner(db).apply_type_mapping_impl(
                    db,
                    mapping,
                    TypeContext::default(),
                    visitor,
                ),
                self.mapping(db).map_types(db, mapping, visitor),
                self.context(db)
                    .with_declared_type(self.context(db).declared_type().map(|ty| {
                        ty.apply_type_mapping_impl(db, mapping, TypeContext::default(), visitor)
                    })),
            ));
        }
        if let Some(mapped) = Self::defer(db, Type::Mapped(self), mapping, context, visitor) {
            return mapped;
        }
        visitor.visit(db, Type::Mapped(self), mapping, || {
            self.expose_with_recursion(db, visitor.env, visitor.recursion_context)
                .map(|exposed| exposed.apply_type_mapping_impl(db, mapping, context, visitor))
                .into_type()
        })
    }

    fn with_inner(self, db: &'db dyn Db, inner: Type<'db>) -> Self {
        Self::new_internal(
            db,
            self.program(db),
            inner,
            self.mapping(db).clone(),
            self.context(db),
        )
    }

    pub(in crate::types) fn variance_equation(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        let env = self.environment(db);
        self.expose(db, &env)
            .map(|exposed| exposed.variance_of(db, &env, typevar))
            .unwrap_or(VarianceTerm::BIVARIANT)
    }

    pub(super) fn recursive_identity(self, db: &'db dyn Db) -> Option<TypeIdentity<'db>> {
        let inner = match self.inner(db) {
            Type::Recursive(recursive) if recursive.may_have_unbounded_specialization(db) => {
                Type::Recursive(recursive.constructor(db))
            }
            Type::TypeAlias(alias)
                if !matches!(
                    Type::TypeAlias(alias).to_type_identity(db),
                    TypeIdentity::Other(_)
                ) =>
            {
                Type::TypeAlias(alias.unspecialized(db))
            }
            Type::Mapped(mapped) => match mapped.recursive_identity(db)? {
                TypeIdentity::Mapped(mapped, _) => Type::Mapped(mapped),
                _ => return None,
            },
            _ => return None,
        };
        Some(TypeIdentity::Mapped(
            self.with_inner(db, inner),
            self.0.argument_matches(db),
        ))
    }

    pub(in crate::types) fn argument_variances(
        self,
        db: &'db dyn Db,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> Box<[TypeVarVariance]> {
        let env = self.environment(db);
        Type::Mapped(self)
            .mapping_arguments(db, &ApplyTypeMappingVisitor::new(&env))
            .into_iter()
            .map(|argument| argument.variance_of(db, &env, typevar).evaluate(db))
            .collect()
    }

    /// Distinguish arguments that can complete an exact-match mapping pattern.
    fn argument_matches(self, db: &'db dyn Db) -> Box<[bool]> {
        let env = self.environment(db);
        let patterns = RefCell::new(FxOrderSet::default());
        let mut operands = Vec::new();
        let mut current = Type::Mapped(self);
        while let Type::Mapped(mapped) = current {
            operands.push(current);
            let selectors: Vec<_> = match mapped.mapping(db) {
                DeferredTypeMapping::RescopeReturnCallables(replacements) => replacements
                    .iter()
                    .map(|(from, _)| Type::Callable(*from))
                    .collect(),
                DeferredTypeMapping::BindSelf(binding) => vec![binding.self_type()],
                _ => Vec::new(),
            };
            for selector in selectors {
                any_over_type_including_alias_arguments(db, &env, selector, |ty| {
                    patterns.borrow_mut().insert(ty);
                    false
                });
            }
            current = mapped.inner(db);
        }
        let patterns = patterns.into_inner();
        if patterns.is_empty() {
            return Box::default();
        }
        operands.push(current);
        let visitor = ApplyTypeMappingVisitor::new(&env);
        operands
            .into_iter()
            .flat_map(|operand| operand.mapping_arguments(db, &visitor))
            .flat_map(|argument| patterns.iter().map(move |pattern| argument == *pattern))
            .collect()
    }

    /// Visit argument dependencies after the deferred transformations, in either polarity.
    pub(super) fn visit_arguments(self, db: &'db dyn Db, visitor: &impl TypeVisitor<'db>) {
        for argument in Type::Mapped(self).mapping_arguments(
            db,
            &ApplyTypeMappingVisitor::new(visitor.program_environment()),
        ) {
            visitor.visit_type(db, argument);
        }
    }

    /// Visit the finite stored operands without evaluating a recursive body.
    pub(super) fn visit_types(self, db: &'db dyn Db, visitor: &impl TypeVisitor<'db>) {
        visitor.visit_type(db, self.inner(db));
        self.mapping(db).visit_types(db, visitor);
        if let Some(context) = self.context(db).declared_type() {
            visitor.visit_type(db, context);
        }
    }

    pub(super) fn mapping_name(self, db: &'db dyn Db) -> &'static str {
        match self.mapping(db) {
            DeferredTypeMapping::ApplySpecialization(_) => "ApplySpecialization",
            DeferredTypeMapping::ApplySpecializationWithMaterialization {
                kind: MaterializationKind::Top,
                ..
            } => "ApplySpecializationWithMaterialization(Top)",
            DeferredTypeMapping::ApplySpecializationWithMaterialization {
                kind: MaterializationKind::Bottom,
                ..
            } => "ApplySpecializationWithMaterialization(Bottom)",
            DeferredTypeMapping::Promote(PromotionMode::On, PromotionKind::Regular) => {
                "Promote(On, Regular)"
            }
            DeferredTypeMapping::Promote(PromotionMode::Off, PromotionKind::Regular) => {
                "Promote(Off, Regular)"
            }
            DeferredTypeMapping::Promote(PromotionMode::On, PromotionKind::SingletonsOnly) => {
                "Promote(On, SingletonsOnly)"
            }
            DeferredTypeMapping::Promote(PromotionMode::Off, PromotionKind::SingletonsOnly) => {
                "Promote(Off, SingletonsOnly)"
            }
            DeferredTypeMapping::Promote(PromotionMode::On, PromotionKind::ClassLiteralsOnly) => {
                "Promote(On, ClassLiteralsOnly)"
            }
            DeferredTypeMapping::Promote(PromotionMode::Off, PromotionKind::ClassLiteralsOnly) => {
                "Promote(Off, ClassLiteralsOnly)"
            }
            DeferredTypeMapping::BindLegacyTypevars(_) => "BindLegacyTypevars",
            DeferredTypeMapping::FreshenBoundTypeVars { .. } => "FreshenBoundTypeVars",
            DeferredTypeMapping::BindSelf(_) => "BindSelf",
            DeferredTypeMapping::ReplaceSelf { .. } => "ReplaceSelf",
            DeferredTypeMapping::Materialize {
                kind: MaterializationKind::Top,
                ..
            } => "Top",
            DeferredTypeMapping::Materialize {
                kind: MaterializationKind::Bottom,
                ..
            } => "Bottom",
            DeferredTypeMapping::ReplaceParameterDefaults => "ReplaceParameterDefaults",
            DeferredTypeMapping::RescopeReturnCallables(_) => "RescopeReturnCallables",
            DeferredTypeMapping::GeneralizeLocalTypes(_) => "GeneralizeLocalTypes",
        }
    }
}

/// Prove identity over the whole recursive component, including both polarities.
/// Backedges retain their original types during the probe; a change anywhere in
/// the component prevents dropping a mapping at an otherwise unchanged node.
#[derive(Default)]
pub(super) struct MappingEffects<'db> {
    active: ActiveRecursionDetector<TypeIdentity<'db>>,
    changed: Cell<bool>,
}

impl<'db> MappingEffects<'db> {
    fn observe(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) {
        if self.changed.get() {
            return;
        }
        // An exact callable can first appear at a later growing specialization.
        if matches!(mapping, TypeMapping::RescopeReturnCallables(replacements) if !replacements.is_empty())
        {
            self.changed.set(true);
            return;
        }
        let visit_arguments = || {
            for argument in ty.mapping_arguments(db, &visitor.without_mapping_probe()) {
                self.inspect(db, argument, mapping, TypeContext::default(), visitor);
            }
        };
        self.active
            .visit(&ty.to_type_identity(db), visit_arguments, || {
                visit_arguments();
                let exposed = match ty {
                    Type::TypeAlias(alias) => UnfoldResult::Unfolded(
                        alias.value_type_with_recursion(db, visitor.recursion_context),
                    ),
                    Type::Mapped(mapped) => {
                        mapped.expose_with_visitor(db, &visitor.without_mapping_probe())
                    }
                    _ => ty.expose(db, visitor.env),
                };
                if let Some(body) = exposed.into_unfolded() {
                    self.inspect(db, body, mapping, context, visitor);
                }
            });
    }

    fn inspect(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        mapping: &TypeMapping<'_, 'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) {
        if !self.changed.get()
            && (ty.apply_type_mapping_impl(db, mapping, context, visitor) != ty
                || ty.apply_type_mapping_impl(db, &mapping.flip(), context, visitor) != ty)
        {
            self.changed.set(true);
        }
    }
}

impl<'db> VarianceInferable<'db> for MappedType<'db> {
    fn variance_of(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarIdentity<'db>,
    ) -> VarianceTerm<'db> {
        VarianceTerm::variable(db, VarianceOrigin::Mapped(self), typevar)
    }
}

impl<'db> Type<'db> {
    /// Conservative argument dependencies after all earlier transformations.
    /// Used by recursion and type-variable analyses, never to construct an exposed body.
    pub(super) fn mapping_arguments(
        self,
        db: &'db dyn Db,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Vec<Type<'db>> {
        let mut mappings = Vec::new();
        let mut inner = self;
        while let Type::Mapped(mapped) = inner {
            mappings.push(mapped.mapping(db));
            inner = mapped.inner(db);
        }
        let mut arguments: Vec<_> = inner
            .recursive_specialization(db)
            .map_or_else(Vec::new, |arguments| {
                let mut types = arguments.types(db).to_vec();
                if let Some(tuple) = arguments.tuple(db) {
                    types.extend(tuple.iter_element_types(db));
                }
                types
            })
            .into_iter()
            .map(|argument| [argument, argument])
            .collect();
        // All transformations see the same occurrence's polarity. Flipping each
        // transformation independently would enumerate impossible combinations.
        for mapping in mappings.into_iter().rev() {
            let flipped = mapping.flip();
            for [positive, negative] in &mut arguments {
                *positive = mapping.apply(db, *positive, TypeContext::default(), visitor);
                *negative = flipped.apply(db, *negative, TypeContext::default(), visitor);
            }
            if let DeferredTypeMapping::RescopeReturnCallables(replacements) = mapping {
                arguments.extend(replacements.iter().map(|(_, to)| [Type::Callable(*to); 2]));
            }
        }
        arguments.into_iter().flatten().collect()
    }

    /// The stored arguments of a recursive expression, before applying deferred mappings.
    pub(super) fn recursive_specialization(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        match self {
            Type::Recursive(recursive) => recursive.arguments(db),
            Type::TypeAlias(alias) => alias.specialization(db),
            Type::Mapped(mapped) => mapped.inner(db).recursive_specialization(db),
            _ => None,
        }
    }

    /// Expose one outer layer of an alias, recursive binder, or deferred mapping.
    pub(crate) fn expose(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> UnfoldResult<'db> {
        match self {
            Type::Recursive(recursive) => recursive.unfold(db),
            Type::Mapped(mapped) => mapped.expose(db, env),
            Type::TypeAlias(alias) => {
                let value = alias.value_type(db);
                if value == self {
                    UnfoldResult::Unchanged(self)
                } else {
                    UnfoldResult::Unfolded(value)
                }
            }
            _ => UnfoldResult::Unchanged(self),
        }
    }
}

/// Owned arguments for a mapping that can outlive the traversal that requested it.
/// Structural substitutions act immediately; eager expansion is an observation,
/// not a transformation to retain at recursive references.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum DeferredTypeMapping<'db> {
    ApplySpecialization(DeferredSpecialization<'db>),
    ApplySpecializationWithMaterialization {
        specialization: DeferredSpecialization<'db>,
        kind: MaterializationKind,
    },
    Promote(PromotionMode, PromotionKind),
    BindLegacyTypevars(BindingContext<'db>),
    FreshenBoundTypeVars {
        generic_context: GenericContext<'db>,
        delta: u32,
    },
    BindSelf(SelfBinding<'db>),
    ReplaceSelf {
        new_origin: SelfTypeVarOrigin<'db>,
        new_upper_bound: Type<'db>,
    },
    Materialize {
        kind: MaterializationKind,
        typevar_bounds_and_defaults: bool,
    },
    ReplaceParameterDefaults,
    RescopeReturnCallables(Box<[(CallableType<'db>, CallableType<'db>)]>),
    GeneralizeLocalTypes(ScopeId<'db>),
}

impl<'db> DeferredTypeMapping<'db> {
    fn flip(&self) -> Self {
        match self {
            Self::Materialize {
                kind,
                typevar_bounds_and_defaults,
            } => Self::Materialize {
                kind: kind.flip(),
                typevar_bounds_and_defaults: *typevar_bounds_and_defaults,
            },
            Self::ApplySpecializationWithMaterialization {
                specialization,
                kind,
            } => Self::ApplySpecializationWithMaterialization {
                specialization: specialization.clone(),
                kind: kind.flip(),
            },
            Self::Promote(mode, kind) => Self::Promote(mode.flip(), *kind),
            _ => self.clone(),
        }
    }

    fn visit_types(&self, db: &'db dyn Db, visitor: &impl TypeVisitor<'db>) {
        match self {
            Self::ApplySpecialization(specialization)
            | Self::ApplySpecializationWithMaterialization { specialization, .. } => {
                specialization.visit_types(db, visitor);
            }
            Self::BindSelf(binding) => visitor.visit_type(db, binding.ty),
            Self::ReplaceSelf {
                new_upper_bound, ..
            } => visitor.visit_type(db, *new_upper_bound),
            Self::RescopeReturnCallables(replacements) => {
                for &(from, to) in replacements {
                    visitor.visit_type(db, Type::Callable(from));
                    visitor.visit_type(db, Type::Callable(to));
                }
            }
            _ => {}
        }
    }

    fn capture(
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Option<Self> {
        Some(match mapping {
            TypeMapping::ApplySpecialization(specialization) => {
                Self::ApplySpecialization(DeferredSpecialization::capture(*specialization))
            }
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            } => Self::ApplySpecializationWithMaterialization {
                specialization: DeferredSpecialization::capture(*specialization),
                kind: *materialization_kind,
            },
            TypeMapping::Promote(mode, kind) => Self::Promote(*mode, *kind),
            TypeMapping::BindLegacyTypevars(context) => Self::BindLegacyTypevars(*context),
            TypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            } => Self::FreshenBoundTypeVars {
                generic_context: *generic_context,
                delta: *delta,
            },
            TypeMapping::BindSelf(binding) => Self::BindSelf(binding.clone()),
            TypeMapping::ReplaceSelf {
                new_origin,
                new_upper_bound,
            } => Self::ReplaceSelf {
                new_origin: *new_origin,
                new_upper_bound: *new_upper_bound,
            },
            TypeMapping::Materialize(kind) => Self::Materialize {
                kind: *kind,
                typevar_bounds_and_defaults: visitor.materialize_typevar_bounds_and_defaults,
            },
            TypeMapping::ReplaceParameterDefaults => Self::ReplaceParameterDefaults,
            TypeMapping::RescopeReturnCallables(replacements) => {
                let mut replacements: Vec<_> =
                    replacements.iter().map(|(&from, &to)| (from, to)).collect();
                replacements.sort_unstable_by_key(|(from, _)| from.as_id());
                Self::RescopeReturnCallables(replacements.into_boxed_slice())
            }
            TypeMapping::GeneralizeLocalTypes(scope) => Self::GeneralizeLocalTypes(*scope),
            TypeMapping::ApplyRecursiveSubstitution(_) | TypeMapping::EagerExpansion => {
                return None;
            }
        })
    }

    fn apply(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        // A traversal cache belongs to one transformation. Composed mappings share
        // equivalence guards, but must not reuse each other's transformed nodes.
        let mut root = visitor.for_new_mapping_root();
        if let Self::Materialize {
            typevar_bounds_and_defaults,
            ..
        } = self
        {
            root.materialize_typevar_bounds_and_defaults = *typevar_bounds_and_defaults;
        }
        let visitor = &root;
        let mapping = match self {
            Self::ApplySpecialization(specialization) => {
                return specialization.apply(db, ty, None, context, visitor);
            }
            Self::ApplySpecializationWithMaterialization {
                specialization,
                kind,
            } => return specialization.apply(db, ty, Some(*kind), context, visitor),
            Self::Promote(mode, kind) => TypeMapping::Promote(*mode, *kind),
            Self::BindLegacyTypevars(binding) => TypeMapping::BindLegacyTypevars(*binding),
            Self::FreshenBoundTypeVars {
                generic_context,
                delta,
            } => TypeMapping::FreshenBoundTypeVars {
                generic_context: *generic_context,
                delta: *delta,
            },
            Self::BindSelf(binding) => TypeMapping::BindSelf(binding.clone()),
            Self::ReplaceSelf {
                new_origin,
                new_upper_bound,
            } => TypeMapping::ReplaceSelf {
                new_origin: *new_origin,
                new_upper_bound: *new_upper_bound,
            },
            Self::Materialize { kind, .. } => TypeMapping::Materialize(*kind),
            Self::ReplaceParameterDefaults => TypeMapping::ReplaceParameterDefaults,
            Self::RescopeReturnCallables(replacements) => {
                let replacements = FxHashMap::from_iter(replacements.iter().copied());
                return ty.apply_type_mapping_impl(
                    db,
                    &TypeMapping::RescopeReturnCallables(&replacements),
                    context,
                    visitor,
                );
            }
            Self::GeneralizeLocalTypes(scope) => TypeMapping::GeneralizeLocalTypes(*scope),
        };
        ty.apply_type_mapping_impl(db, &mapping, context, visitor)
    }

    fn map_types(
        &self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        match self {
            Self::ApplySpecialization(specialization) => {
                Self::ApplySpecialization(specialization.map_types(db, mapping, visitor))
            }
            Self::ApplySpecializationWithMaterialization {
                specialization,
                kind,
            } => Self::ApplySpecializationWithMaterialization {
                specialization: specialization.map_types(db, mapping, visitor),
                kind: *kind,
            },
            Self::BindSelf(binding) => Self::BindSelf(SelfBinding {
                ty: binding.ty.apply_type_mapping_impl(
                    db,
                    mapping,
                    TypeContext::default(),
                    visitor,
                ),
                ..binding.clone()
            }),
            Self::ReplaceSelf {
                new_origin,
                new_upper_bound,
            } => Self::ReplaceSelf {
                new_origin: *new_origin,
                new_upper_bound: new_upper_bound.apply_type_mapping_impl(
                    db,
                    mapping,
                    TypeContext::default(),
                    visitor,
                ),
            },
            Self::RescopeReturnCallables(replacements) => Self::RescopeReturnCallables(
                replacements
                    .iter()
                    .map(|(from, to)| {
                        (
                            from.apply_type_mapping_impl(
                                db,
                                mapping,
                                TypeContext::default(),
                                visitor,
                            ),
                            to.apply_type_mapping_impl(
                                db,
                                mapping,
                                TypeContext::default(),
                                visitor,
                            ),
                        )
                    })
                    .collect(),
            ),
            _ => self.clone(),
        }
    }
}

/// Owned substitutions retained by deferred type mappings.
#[derive(Clone, Debug, PartialEq, Eq, Hash, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum DeferredSpecialization<'db> {
    Specialization {
        specialization: Specialization<'db>,
        specialize_self_domain: bool,
    },
    TypeAlias(Specialization<'db>),
    Partial {
        generic_context: GenericContext<'db>,
        types: Box<[Type<'db>]>,
        skip: Option<usize>,
    },
    ReturnCallables(Box<[(BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>)]>),
    Single(BoundTypeVarInstance<'db>, Type<'db>),
}

impl<'db> DeferredSpecialization<'db> {
    fn visit_types(&self, db: &'db dyn Db, visitor: &impl TypeVisitor<'db>) {
        match self {
            Self::Specialization { specialization, .. } | Self::TypeAlias(specialization) => {
                walk_specialization_types(db, *specialization, visitor);
            }
            Self::Partial { types, .. } => {
                for &ty in types {
                    visitor.visit_type(db, ty);
                }
            }
            Self::Single(_, ty) => visitor.visit_type(db, *ty),
            Self::ReturnCallables(replacements) => {
                for &(from, to) in replacements {
                    visitor.visit_type(db, Type::TypeVar(from));
                    visitor.visit_type(db, Type::TypeVar(to));
                }
            }
        }
    }

    fn capture(specialization: ApplySpecialization<'_, 'db>) -> Self {
        match specialization {
            ApplySpecialization::Specialization {
                specialization,
                specialize_self_domain,
            } => Self::Specialization {
                specialization,
                specialize_self_domain,
            },
            ApplySpecialization::TypeAlias(specialization) => Self::TypeAlias(specialization),
            ApplySpecialization::Partial {
                generic_context,
                types,
                skip,
            } => Self::Partial {
                generic_context,
                types: types.into(),
                skip,
            },
            ApplySpecialization::ReturnCallables(replacements) => {
                let mut replacements: Vec<_> =
                    replacements.iter().map(|(&from, &to)| (from, to)).collect();
                replacements.sort_unstable_by_key(|(from, _)| from.as_id());
                Self::ReturnCallables(replacements.into_boxed_slice())
            }
            ApplySpecialization::Single(variable, ty) => Self::Single(variable, ty),
            ApplySpecialization::WithBindings { specialization, .. } => {
                Self::capture(*specialization)
            }
        }
    }

    fn apply(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        materialization_kind: Option<MaterializationKind>,
        context: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        let replacements;
        let specialization = match self {
            Self::Specialization {
                specialization,
                specialize_self_domain,
            } => ApplySpecialization::Specialization {
                specialization: *specialization,
                specialize_self_domain: *specialize_self_domain,
            },
            Self::TypeAlias(specialization) => ApplySpecialization::TypeAlias(*specialization),
            Self::Partial {
                generic_context,
                types,
                skip,
            } => ApplySpecialization::Partial {
                generic_context: *generic_context,
                types,
                skip: *skip,
            },
            Self::ReturnCallables(pairs) => {
                replacements = FxIndexMap::from_iter(pairs.iter().copied());
                ApplySpecialization::ReturnCallables(&replacements)
            }
            Self::Single(variable, ty) => ApplySpecialization::Single(*variable, *ty),
        };
        let mapping = match materialization_kind {
            None => TypeMapping::ApplySpecialization(specialization),
            Some(materialization_kind) => TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            },
        };
        ty.apply_type_mapping_impl(db, &mapping, context, visitor)
    }

    fn map_types(
        &self,
        db: &'db dyn Db,
        mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        match self {
            Self::Specialization {
                specialization,
                specialize_self_domain,
            } => Self::Specialization {
                specialization: specialization.apply_type_mapping_impl(db, mapping, &[], visitor),
                specialize_self_domain: *specialize_self_domain,
            },
            Self::TypeAlias(specialization) => {
                Self::TypeAlias(specialization.apply_type_mapping_impl(db, mapping, &[], visitor))
            }
            Self::Partial {
                generic_context,
                types,
                skip,
            } => Self::Partial {
                generic_context: *generic_context,
                types: types
                    .iter()
                    .map(|ty| {
                        ty.apply_type_mapping_impl(db, mapping, TypeContext::default(), visitor)
                    })
                    .collect(),
                skip: *skip,
            },
            Self::Single(variable, ty) => Self::Single(
                *variable,
                ty.apply_type_mapping_impl(db, mapping, TypeContext::default(), visitor),
            ),
            Self::ReturnCallables(_) => self.clone(),
        }
    }
}
