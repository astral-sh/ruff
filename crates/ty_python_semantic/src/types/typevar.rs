use crate::ProgramEnvironment;
use crate::types::cyclic::CycleIdentityMode;
use crate::types::mapping::effects::{
    InlineMappingEffects, MappingEffects, MappingOperation, MappingStartEffects, MappingWork,
    inline_mapping_result,
};
use crate::types::mapping::{MappingStart, TypeVarMappingContinuation};
use crate::types::signatures::effects::legacy_inline;
use crate::types::typevar::specialization::{
    OrdinaryTypeVarSpecialization, TypeVarSpecialization, TypeVarSpecializationFacts,
    specialize_bound_typevar_sync,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use itertools::{Either, Itertools};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use salsa::execution_probe::{FieldRequest, FieldRequestContext};
#[cfg(any(test, feature = "experimental-analysis"))]
use salsa::plumbing::function::IngredientImpl;

use crate::{
    Db, FxOrderMap, TypeQualifiers,
    place::{
        DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, PublicTypePolicy,
        TypeOrigin,
    },
    types::{
        ApplySpecialization, ApplyTypeMappingVisitor, CycleDetector, DynamicType, GenericContext,
        InstanceProjection, IntersectionType, KnownClass, KnownInstanceType, MaterializationKind,
        Parameters, Specialization, Type, TypeContext, TypeMapping, TypeVarVariance, UnionBuilder,
        UnionType, any_over_type, any_over_type_including_alias_arguments, binding_type,
        definition_expression_type,
        tuple::Tuple,
        variance::VarianceInferable,
        visitor::{self, TypeCollector, TypeVisitor, walk_type_with_recursion_guard},
    },
};
use ty_python_core::{
    Program,
    definition::{Definition, DefinitionKind},
    semantic_index,
};

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) mod runtime;

pub(in crate::types) mod bounds;
pub(in crate::types) mod construction;
pub(in crate::types) mod constructor_nonce;
pub(in crate::types) mod default;
pub(in crate::types) mod freshening;
pub(in crate::types) mod retained_self;
pub(in crate::types) mod name_suffix;
pub(in crate::types) mod specialization;

impl<'db> Type<'db> {
    pub(crate) const fn is_type_var(self) -> bool {
        matches!(self, Type::TypeVar(_))
    }

    pub(crate) const fn as_typevar(self) -> Option<BoundTypeVarInstance<'db>> {
        match self {
            Type::TypeVar(bound_typevar) => Some(bound_typevar),
            _ => None,
        }
    }

    pub(crate) fn has_typevar(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        any_over_type(db, env, self, false, |ty| matches!(ty, Type::TypeVar(_)))
    }

    pub(crate) fn references_typevar(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar_id: TypeVarIdentity<'db>,
    ) -> bool {
        any_over_type(db, env, self, false, |ty| match ty {
            Type::TypeVar(bound_typevar) => typevar_id == bound_typevar.typevar(db).identity(db),
            Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => {
                typevar_id == typevar.identity(db)
            }
            _ => false,
        })
    }

    /// Returns whether this type might reference `typevar_id`, including type-alias arguments.
    ///
    /// Other non-lazy type-variable visitors stop at type aliases because inspecting an alias's
    /// value can trigger lazy inference or expand a recursive definition. Receiver specialization
    /// still needs to notice `T` in `Alias[T]`, so this visitor inspects the already-available
    /// specialization arguments without evaluating the alias body.
    ///
    /// This deliberately over-approximates: `type Alias[T] = int` does not actually depend on
    /// `T`, and specialization can also erase an argument. That can cause an unnecessary
    /// receiver-specialization attempt, but actual receiver constraints are still solved before
    /// changing the signature. Applying the same traversal to visitors that use type-variable
    /// occurrences to drive inference or diagnostics can instead change behavior.
    ///
    /// TODO: Explore whether other type-variable visitors can safely inspect alias arguments,
    /// accounting for unused parameters and arguments erased by specialization.
    pub(crate) fn references_typevar_through_aliases(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar_id: TypeVarIdentity<'db>,
    ) -> bool {
        any_over_type_including_alias_arguments(db, env, self, |ty| match ty {
            Type::TypeVar(typevar) => typevar_id == typevar.typevar(db).identity(db),
            Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => {
                typevar_id == typevar.identity(db)
            }
            _ => false,
        })
    }

    pub(crate) fn has_non_self_typevar(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        any_over_type(
            db,
            env,
            self,
            false,
            |ty| matches!(ty, Type::TypeVar(tv) if !tv.typevar(db).is_self(db)),
        )
    }

    pub(crate) fn has_typevar_or_typevar_instance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        any_over_type(db, env, self, false, |ty| {
            matches!(
                ty,
                Type::KnownInstance(KnownInstanceType::TypeVar(_)) | Type::TypeVar(_)
            )
        })
    }

    pub(crate) fn has_unspecialized_type_var(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        // Contextual inference must not adopt an alias whose arguments still
        // contain placeholders from an enclosing generic call.
        any_over_type_including_alias_arguments(db, env, self, |ty| {
            matches!(ty, Type::Dynamic(DynamicType::UnspecializedTypeVar))
        })
    }

    pub(crate) fn has_provisional_marker(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
        any_over_type(db, env, self, false, |ty| {
            ty.as_dynamic()
                .is_some_and(DynamicType::is_provisional_marker)
        })
    }
}

/// A specific instance of a type variable that has not been bound to a generic context yet.
///
/// This is usually not the type that you want; if you are working with a typevar, in a generic
/// context, which might be specialized to a concrete type, you want [`BoundTypeVarInstance`]. This
/// type holds information that does not depend on which generic context the typevar is used in.
///
/// For a legacy typevar:
///
/// ```py
/// T = TypeVar("T")                       # [1]
/// def generic_function(t: T) -> T: ...   # [2]
/// ```
///
/// we will create a `TypeVarInstance` for the typevar `T` when it is instantiated. The type of `T`
/// at `[1]` will be a `KnownInstanceType::TypeVar` wrapping this `TypeVarInstance`. The typevar is
/// not yet bound to any generic context at this point.
///
/// The typevar is used in `generic_function`, which binds it to a new generic context. We will
/// create a [`BoundTypeVarInstance`] for this new binding of the typevar. The type of `T` at `[2]`
/// will be a `Type::TypeVar` wrapping this `BoundTypeVarInstance`.
///
/// For a PEP 695 typevar:
///
/// ```py
/// def generic_function[T](t: T) -> T: ...
/// #                          ╰─────╰─────────── [2]
/// #                    ╰─────────────────────── [1]
/// ```
///
/// the typevar is defined and immediately bound to a single generic context. Just like in the
/// legacy case, we will create a `TypeVarInstance` and [`BoundTypeVarInstance`], and the type of
/// `T` at `[1]` and `[2]` will be that `TypeVarInstance` and `BoundTypeVarInstance`, respectively.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeVarInstance<'db> {
    /// The identity of this typevar
    #[returns(copy)]
    pub(crate) identity: TypeVarIdentity<'db>,

    /// The upper bound or constraint on the type of this TypeVar, if any. Don't use this field
    /// directly; use the `bound_or_constraints` (or `upper_bound` and `constraints`) methods
    /// instead (to evaluate any lazy bound or constraints).
    #[returns(copy)]
    _bound_or_constraints: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,

    /// The explicitly specified variance of the TypeVar
    #[returns(copy)]
    pub(super) explicit_variance: Option<TypeVarVariance>,

    /// The default type for this TypeVar, if any. Don't use this field directly to obtain the type;
    /// use the `default_type` method instead (to evaluate any lazy default). Metadata-preserving
    /// reconstruction may copy this stored descriptor without evaluating it.
    #[returns(copy)]
    _default: Option<TypeVarDefaultEvaluation<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeVarInstance<'_> {}

pub(super) fn walk_type_var_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typevar: TypeVarInstance<'db>,
    visitor: &V,
) {
    let include_lazy = visitor.should_visit_lazy_type_attributes();
    let (bounds, skipped_lazy) =
        typevar.bounds_for_visitor(db, visitor.program_environment(), include_lazy);
    if skipped_lazy {
        visitor.notify_skipped_lazy_type_attributes();
    }
    if let Some(bound_or_constraints) = bounds {
        walk_type_var_bounds(db, bound_or_constraints, visitor);
    }
    let include_lazy = visitor.should_visit_lazy_type_attributes();
    let (default_type, skipped_lazy) =
        typevar.default_for_visitor(db, visitor.program_environment(), include_lazy);
    if skipped_lazy {
        visitor.notify_skipped_lazy_type_attributes();
    }
    if let Some(default_type) = default_type {
        visitor.visit_type(db, default_type);
    }
}

#[salsa::tracked]
impl<'db> TypeVarInstance<'db> {
    pub(in crate::types) fn bound_or_constraints_request(
        self,
        context: FieldRequestContext<'db>,
    ) -> impl FieldRequest<
        'db,
        Stored = Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        Output = Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
    > {
        self.field_requests(context)._bound_or_constraints()
    }

    pub(in crate::types) fn default_request(
        self,
        context: FieldRequestContext<'db>,
    ) -> impl FieldRequest<
        'db,
        Stored = Option<TypeVarDefaultEvaluation<'db>>,
        Output = Option<TypeVarDefaultEvaluation<'db>>,
    > {
        self.field_requests(context)._default()
    }

    pub(super) fn bounds_for_visitor(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        include_lazy: bool,
    ) -> (Option<TypeVarBoundOrConstraints<'db>>, bool) {
        if include_lazy {
            return (self.bound_or_constraints(db, env), false);
        }
        self.eager_bounds_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn eager_bounds_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> (Option<TypeVarBoundOrConstraints<'db>>, bool) {
        decode_eager_bounds(*self.read_fields(fields)._bound_or_constraints())
    }

    pub(super) fn default_for_visitor(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        include_lazy: bool,
    ) -> (Option<Type<'db>>, bool) {
        if include_lazy {
            return (self.default_type(db, env), false);
        }
        self.eager_default_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn eager_default_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> (Option<Type<'db>>, bool) {
        decode_eager_default(*self.read_fields(fields)._default())
    }

    pub(crate) fn with_binding_context(
        self,
        db: &'db dyn Db,
        binding_context: Definition<'db>,
    ) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::new(
            db,
            self,
            BindingContext::Definition(binding_context),
            None,
            TypeVarNonce::NONE,
        )
    }

    pub(super) fn with_identity(self, db: &'db dyn Db, identity: TypeVarIdentity<'db>) -> Self {
        Self::new(
            db,
            identity,
            self._bound_or_constraints(db),
            self.explicit_variance(db),
            self._default(db),
        )
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db Name {
        self.identity(db).name(db)
    }

    pub(crate) fn definition(self, db: &'db dyn Db) -> Option<Definition<'db>> {
        self.identity(db).definition(db)
    }

    pub fn kind(self, db: &'db dyn Db) -> TypeVarKind {
        self.identity(db).kind(db)
    }

    pub(crate) fn is_self(self, db: &'db dyn Db) -> bool {
        matches!(self.kind(db), TypeVarKind::TypingSelf)
    }

    pub(crate) fn is_paramspec(self, db: &'db dyn Db) -> bool {
        self.kind(db).is_paramspec()
    }

    pub(crate) fn is_typevartuple(self, db: &'db dyn Db) -> bool {
        self.kind(db).is_typevartuple()
    }

    pub(crate) fn upper_bound(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        if let Some(TypeVarBoundOrConstraints::UpperBound(ty)) = self.bound_or_constraints(db, env)
        {
            Some(ty)
        } else {
            None
        }
    }

    /// Returns whether this type variable has constraints without evaluating a lazy bound.
    pub(super) fn is_constrained(self, db: &'db dyn Db) -> bool {
        matches!(
            self._bound_or_constraints(db),
            Some(
                TypeVarBoundOrConstraintsEvaluation::Eager(TypeVarBoundOrConstraints::Constraints(
                    _
                )) | TypeVarBoundOrConstraintsEvaluation::LazyConstraints
            )
        )
    }

    pub(crate) fn constraints(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<&'db [Type<'db>]> {
        if let Some(TypeVarBoundOrConstraints::Constraints(tuple)) =
            self.bound_or_constraints(db, env)
        {
            Some(tuple.elements(db))
        } else {
            None
        }
    }

    pub(crate) fn bound_or_constraints(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<TypeVarBoundOrConstraints<'db>> {
        match bounds::typevar_bounds_sync(self, env, &bounds::OrdinaryTypeVarBoundsEffects { db }) {
            Ok(bounds) => bounds,
            Err(never) => match never {},
        }
    }

    pub(crate) fn default_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        self.default_type_impl(db, env, None)
    }

    fn default_type_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        visitor: Option<&TypeVarDefaultVisitor<'db>>,
    ) -> Option<Type<'db>> {
        let Ok(default) = default::evaluation::typevar_default_sync(
            self,
            env,
            self._default(db),
            &default::evaluation::OrdinaryTypeVarDefaultEffects { db, visitor },
        );
        default
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        Self::new(
            db,
            self.identity(db),
            self._bound_or_constraints(db)
                .and_then(|bound_or_constraints| match bound_or_constraints {
                    TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints) => Some(
                        bound_or_constraints
                            .materialize_impl(db, materialization_kind, visitor)
                            .into(),
                    ),
                    TypeVarBoundOrConstraintsEvaluation::LazyUpperBound => {
                        self.lazy_bound(db, visitor.env).map(|bound| {
                            TypeVarBoundOrConstraints::UpperBound(bound)
                                .materialize_impl(db, materialization_kind, visitor)
                                .into()
                        })
                    }
                    TypeVarBoundOrConstraintsEvaluation::LazyConstraints => {
                        self.lazy_constraints(db, visitor.env).map(|constraints| {
                            TypeVarBoundOrConstraints::Constraints(constraints)
                                .materialize_impl(db, materialization_kind, visitor)
                                .into()
                        })
                    }
                }),
            self.explicit_variance(db),
            self._default(db).and_then(|default| match default {
                TypeVarDefaultEvaluation::Eager(ty) => {
                    Some(ty.materialize(db, materialization_kind, visitor).into())
                }
                TypeVarDefaultEvaluation::Lazy => self
                    .lazy_default(db, visitor.env)
                    .map(|ty| ty.materialize(db, materialization_kind, visitor).into()),
            }),
        )
    }

    fn to_instance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<InstanceProjection<Self>> {
        let bound_or_constraints = match self.bound_or_constraints(db, env)? {
            TypeVarBoundOrConstraints::UpperBound(upper_bound) => upper_bound
                .to_instance(db, env)?
                .map(TypeVarBoundOrConstraints::UpperBound),
            TypeVarBoundOrConstraints::Constraints(constraints) => constraints
                .to_instance(db, env)?
                .map(TypeVarBoundOrConstraints::Constraints),
        };
        let identity = TypeVarIdentity::new(
            db,
            Name::concat(&[self.name(db).as_str(), "'instance"]),
            None, // definition
            self.kind(db),
        );
        Some(bound_or_constraints.map(|bound_or_constraints| {
            Self::new(
                db,
                identity,
                Some(bound_or_constraints.into()),
                self.explicit_variance(db),
                None, // _default
            )
        }))
    }

    fn type_is_self_referential(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        visitor: &TypeVarDefaultVisitor<'db>,
    ) -> bool {
        let Ok(result) = default::self_reference::type_is_self_referential_sync(
            self,
            ty,
            &default::self_reference::OrdinarySelfReferenceEffects { db, env, visitor },
        );
        result
    }

    /// Returns the "unchecked" upper bound of a type variable instance.
    /// `lazy_bound` checks if the upper bound type is generic (generic upper bound is not allowed).
    fn lazy_bound_unchecked(self, db: &'db dyn Db) -> Option<Type<'db>> {
        lazy_bound_unchecked(db, self)
    }

    fn lazy_bound(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        let bound = self.lazy_bound_unchecked(db)?;

        if bound.has_typevar_or_typevar_instance(db, env) {
            return None;
        }

        Some(bound)
    }

    /// Returns the "unchecked" constraints of a type variable instance.
    /// `lazy_constraints` checks if any of the constraint types are generic (generic constraints are not allowed).
    fn lazy_constraints_unchecked(self, db: &'db dyn Db) -> Option<TypeVarConstraints<'db>> {
        lazy_constraints_unchecked(db, self)
    }

    fn lazy_constraints(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<TypeVarConstraints<'db>> {
        let constraints = self.lazy_constraints_unchecked(db)?;

        if constraints
            .elements(db)
            .iter()
            .any(|ty| ty.has_typevar_or_typevar_instance(db, env))
        {
            return None;
        }

        Some(constraints)
    }

    /// Returns the "unchecked" default type of a type variable instance.
    /// `lazy_default` checks if the default type is not self-referential.
    fn lazy_default_unchecked(self, db: &'db dyn Db) -> Option<Type<'db>> {
        lazy_default_unchecked(db, self)
    }

    fn lazy_default(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        let visitor = TypeVarDefaultVisitor::new(None);
        self.lazy_default_impl(db, env, &visitor)
    }

    fn lazy_default_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        visitor: &TypeVarDefaultVisitor<'db>,
    ) -> Option<Type<'db>> {
        let Ok(default) = default::evaluation::lazy_typevar_default_sync(
            self,
            env,
            &default::evaluation::OrdinaryLazyTypeVarDefaultEffects { db, visitor },
        );
        default
    }

    pub fn bind_pep695(self, db: &'db dyn Db) -> Option<BoundTypeVarInstance<'db>> {
        if !matches!(
            self.identity(db).kind(db),
            TypeVarKind::Pep695TypeVar | TypeVarKind::Pep695ParamSpec
        ) {
            return None;
        }
        let typevar_definition = self.definition(db)?;
        let index = semantic_index(db, typevar_definition.program_file(db));
        let (_, child) = index
            .child_scopes(typevar_definition.file_scope(db))
            .next()?;
        GenericContext::of_node(db, child.node(), index)?.binds_typevar(db, self)
    }
}

/// A nonce that gives a bound typevar occurrence a fresh identity.
///
/// `0` is reserved for source-level, non-freshened typevars. Positive values identify fresh
/// occurrences.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct TypeVarNonce(u32);

// This type does not have any heap storage.
impl get_size2::GetSize for TypeVarNonce {}

impl TypeVarNonce {
    pub(crate) const NONE: Self = Self(0);
    const FIRST: Self = Self(1);

    pub(crate) const fn value(self) -> u32 {
        self.0
    }

    pub(crate) fn increment(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("exhausted bound typevar freshness nonces"),
        )
    }

    fn add(self, delta: u32) -> Self {
        Self(
            self.0
                .checked_add(delta)
                .expect("exhausted bound typevar freshness nonces"),
        )
    }
}

#[derive(Debug)]
struct TypeVarNonceGeneratorInner<'db> {
    next: TypeVarNonce,
    seen: FxHashSet<GenericContext<'db>>,
    enclosing: FxHashSet<BindingContext<'db>>,
}

/// A clone-safe generator of fresh bound-typevar occurrence nonces.
///
/// The generator allocates a nonce for the second and later occurrence of a generic context,
/// or on its first occurrence if it is nonempty and all its variables belong to one recorded enclosing context.
/// Other first occurrences can use their source-level identity directly because there is no previous
/// occurrence for them to collide with.
#[derive(Clone, Debug)]
pub(crate) struct TypeVarNonceGenerator<'db> {
    inner: Rc<RefCell<TypeVarNonceGeneratorInner<'db>>>,
}

#[cfg(test)]
thread_local! {
    static OWNERSHIP_PROBE_NONCES: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

#[cfg(test)]
pub(crate) fn ownership_probe_nonce_counts() -> (usize, usize) {
    OWNERSHIP_PROBE_NONCES.get()
}

impl Default for TypeVarNonceGenerator<'_> {
    fn default() -> Self {
        #[cfg(test)]
        OWNERSHIP_PROBE_NONCES.with(|counts| {
            let (starts, allocations) = counts.get();
            counts.set((starts + 1, allocations));
        });
        Self {
            inner: Rc::new(RefCell::new(TypeVarNonceGeneratorInner {
                next: TypeVarNonce::FIRST,
                seen: FxHashSet::default(),
                enclosing: FxHashSet::default(),
            })),
        }
    }
}

impl<'db> TypeVarNonceGenerator<'db> {
    /// Records enclosing contexts whose variables must be freshened even on their first occurrence.
    pub(crate) fn record_enclosing_binding_contexts(
        &self,
        binding_contexts: impl IntoIterator<Item = BindingContext<'db>>,
    ) {
        let mut inner = self.inner.borrow_mut();
        inner.enclosing.extend(binding_contexts);
    }

    /// Decides whether a nonempty context belongs wholly to one enclosing context or has occurred before.
    /// The enclosing-context case leaves the seen set unchanged; other calls record the context.
    pub(crate) fn should_freshen(
        &self,
        db: &'db dyn Db,
        generic_context: GenericContext<'db>,
    ) -> bool {
        legacy_inline(self.should_freshen_with(
            db,
            generic_context,
            &constructor_nonce::InlineConstructorNonceEffects,
        ))
    }

    pub(crate) fn next(&self) -> TypeVarNonce {
        legacy_inline(self.next_with(&constructor_nonce::InlineConstructorNonceEffects))
    }
}

pub(crate) fn max_typevar_freshness_matching_generic_context<'db>(
    db: &'db dyn Db,
    types: impl IntoIterator<Item = Type<'db>>,
    generic_context: GenericContext<'db>,
) -> Option<TypeVarNonce> {
    struct MatchingFreshnessCollector<'a, 'db> {
        env: &'a ProgramEnvironment<'db>,
        base_identities: FxHashSet<BoundTypeVarIdentity<'db>>,
        recursion_guard: TypeCollector<'db>,
        max_freshness: Cell<Option<TypeVarNonce>>,
    }

    impl<'a, 'db> MatchingFreshnessCollector<'a, 'db> {
        fn new(
            db: &'db dyn Db,
            env: &'a ProgramEnvironment<'db>,
            generic_context: GenericContext<'db>,
        ) -> Self {
            let base_identities = generic_context
                .variables(db)
                .map(|typevar| {
                    let mut identity = typevar.identity(db);
                    identity.freshness = TypeVarNonce::NONE;
                    identity
                })
                .collect();
            Self {
                env,
                base_identities,
                recursion_guard: TypeCollector::default(),
                max_freshness: Cell::default(),
            }
        }
    }

    impl<'db> TypeVisitor<'db> for MatchingFreshnessCollector<'_, 'db> {
        fn program_environment(&self) -> &ProgramEnvironment<'db> {
            self.env
        }

        fn should_visit_lazy_type_attributes(&self) -> bool {
            false
        }

        fn visit_bound_type_var_type(
            &self,
            db: &'db dyn Db,
            bound_typevar: BoundTypeVarInstance<'db>,
        ) {
            let mut identity = bound_typevar.identity(db);
            identity.freshness = TypeVarNonce::NONE;
            if self.base_identities.contains(&identity) {
                self.max_freshness.set(
                    self.max_freshness
                        .get()
                        .max(Some(bound_typevar.freshness(db))),
                );
            }
        }

        fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
            walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
        }
    }

    let env = ProgramEnvironment::from_program(generic_context.program(db));
    let collector = MatchingFreshnessCollector::new(db, &env, generic_context);
    for ty in types {
        collector.visit_type(db, ty);
    }
    collector.max_freshness.get()
}

/// A type variable that has been bound to a generic context, and which can be specialized to a
/// concrete type.
#[salsa::interned(field_view = read_fields,
    field_requests = field_requests,
    debug,
    constructor = new_internal,
    heap_size = ruff_memory_usage::heap_size
)]
pub struct BoundTypeVarInstance<'db> {
    #[returns(copy)]
    pub typevar: TypeVarInstance<'db>,
    // This duplicates the source-level identity accessible through `typevar`, but keeps
    // `identity()` to a single interned-field read. Storing only the occurrence-specific fields
    // and reconstructing the full identity regresses hot-path project benchmarks.
    #[returns(copy)]
    identity_inner: BoundTypeVarIdentity<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundTypeVarInstance<'_> {}

impl<'db> BoundTypeVarInstance<'db> {
    pub(in crate::types) fn identity_request(
        self,
        context: FieldRequestContext<'db>,
    ) -> impl FieldRequest<'db, Stored = BoundTypeVarIdentity<'db>, Output = BoundTypeVarIdentity<'db>>
    {
        self.field_requests(context).identity_inner()
    }

    pub(crate) fn new(
        db: &'db dyn Db,
        typevar: TypeVarInstance<'db>,
        binding_context: BindingContext<'db>,
        paramspec_attr: Option<ParamSpecAttrKind>,
        freshness: TypeVarNonce,
    ) -> Self {
        let identity = BoundTypeVarIdentity::new(
            typevar.identity(db),
            binding_context,
            paramspec_attr,
            freshness,
        );
        Self::new_internal(db, typevar, identity)
    }

    pub(super) fn binding_context(self, db: &'db dyn Db) -> BindingContext<'db> {
        self.identity(db).binding_context
    }

    pub(super) fn paramspec_attr(self, db: &'db dyn Db) -> Option<ParamSpecAttrKind> {
        self.identity(db).paramspec_attr
    }

    pub(super) fn freshness(self, db: &'db dyn Db) -> TypeVarNonce {
        self.identity(db).freshness
    }

    pub(crate) fn with_name_suffix(self, db: &'db dyn Db, suffix: &str) -> Self {
        name_suffix::with_name_suffix(db, self, suffix)
    }

    /// Get the identity of this bound typevar occurrence.
    ///
    /// This includes the source-level typevar, binding context, `ParamSpec` attribute, and
    /// freshness nonce. It is used for comparing whether two bound typevars represent the same
    /// occurrence, regardless of e.g. differences in their bounds or constraints due to
    /// materialization.
    pub(crate) fn identity(self, db: &'db dyn Db) -> BoundTypeVarIdentity<'db> {
        self.identity_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn identity_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> BoundTypeVarIdentity<'db> {
        *self.read_fields(fields).identity_inner()
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db Name {
        self.typevar(db).name(db)
    }

    pub(crate) fn kind(self, db: &'db dyn Db) -> TypeVarKind {
        self.identity(db).kind(db)
    }

    pub(crate) fn domain(self, db: &'db dyn Db) -> TypeVarDomain {
        self.domain_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn domain_with_fields(
        self,
        fields: salsa::FieldReads<'db>,
    ) -> TypeVarDomain {
        let identity = self.identity_with_fields(fields);
        let kind = identity.kind_with_fields(fields);
        if kind.is_paramspec() && identity.paramspec_attr.is_none() {
            TypeVarDomain::ParameterSignature
        } else if kind.is_typevartuple() {
            TypeVarDomain::TypeTuple
        } else {
            TypeVarDomain::Type
        }
    }

    pub(crate) fn is_paramspec(self, db: &'db dyn Db) -> bool {
        self.is_paramspec_with_fields(salsa::FieldReads::new(db))
    }

    pub(in crate::types) fn is_paramspec_with_fields(self, fields: salsa::FieldReads<'db>) -> bool {
        self.identity_with_fields(fields)
            .kind_with_fields(fields)
            .is_paramspec()
    }

    pub(crate) fn is_typevartuple(self, db: &'db dyn Db) -> bool {
        self.kind(db).is_typevartuple()
    }

    /// Returns the bounds or constraints of this typevar. If the typevar is unbounded, returns
    /// `object` as its upper bound.
    pub(crate) fn require_bound_or_constraints(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> TypeVarBoundOrConstraints<'db> {
        self.typevar(db)
            .bound_or_constraints(db, env)
            .unwrap_or_else(|| TypeVarBoundOrConstraints::UpperBound(self.domain(db).top(db)))
    }

    /// Returns a new bound typevar instance with the given `ParamSpec` attribute set.
    ///
    /// This method will also set an appropriate upper bound on the typevar, based on the
    /// attribute kind. For `P.args`, the upper bound will be `tuple[object, ...]`, and for
    /// `P.kwargs`, the upper bound will be `Top[dict[str, Any]]`.
    ///
    /// It's the caller's responsibility to ensure that this method is only called on a `ParamSpec`
    /// type variable.
    pub(crate) fn with_paramspec_attr(self, db: &'db dyn Db, kind: ParamSpecAttrKind) -> Self {
        debug_assert!(
            self.is_paramspec(db),
            "Expected a ParamSpec, got {:?}",
            self.kind(db)
        );

        let env = ProgramEnvironment::from_program(self.binding_context(db).program(db));
        let upper_bound = TypeVarBoundOrConstraints::UpperBound(match kind {
            ParamSpecAttrKind::Args => Type::homogeneous_tuple(db, &env, Type::object()),
            ParamSpecAttrKind::Kwargs => KnownClass::Dict
                .to_specialized_instance(
                    db,
                    &env,
                    &[KnownClass::Str.to_instance(db, &env), Type::any()],
                )
                .top_materialization(db, &env),
        });

        let typevar = self.typevar(db);
        let typevar = TypeVarInstance::new(
            db,
            typevar.identity(db),
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(upper_bound)),
            typevar.explicit_variance(db),
            None, // `P.args` and `P.kwargs` cannot have defaults even though `P` can
        );

        Self::new(
            db,
            typevar,
            self.binding_context(db),
            Some(kind),
            self.freshness(db),
        )
    }

    /// Returns a new bound typevar instance without any `ParamSpec` attribute set.
    ///
    /// This method will also remove any upper bound that was set by `with_paramspec_attr`. This
    /// means that the returned typevar will have no upper bound or constraints.
    ///
    /// It's the caller's responsibility to ensure that this method is only called on a `ParamSpec`
    /// type variable.
    pub(crate) fn without_paramspec_attr(self, db: &'db dyn Db) -> Self {
        debug_assert!(
            self.is_paramspec(db),
            "Expected a ParamSpec, got {:?}",
            self.kind(db)
        );

        let typevar = self.typevar(db);
        Self::new(
            db,
            TypeVarInstance::new(
                db,
                typevar.identity(db),
                None, // Remove the upper bound set by `with_paramspec_attr`
                typevar.explicit_variance(db),
                None, // `P.args` and `P.kwargs` cannot have defaults even though `P` can
            ),
            self.binding_context(db),
            None,
            self.freshness(db),
        )
    }

    /// Returns whether two bound typevars represent the same occurrence, regardless of e.g.
    /// differences in their bounds or constraints due to materialization.
    pub(crate) fn is_same_typevar_as(self, db: &'db dyn Db, other: Self) -> bool {
        self.identity(db) == other.identity(db)
    }

    /// Create a new PEP 695 type variable that can be used in signatures
    /// of synthetic generic functions.
    pub(crate) fn synthetic(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: Name,
        variance: TypeVarVariance,
    ) -> Self {
        let identity = TypeVarIdentity::new(
            db,
            name,
            None, // definition
            TypeVarKind::Pep695TypeVar,
        );
        let typevar = TypeVarInstance::new(
            db,
            identity,
            None, // _bound_or_constraints
            Some(variance),
            None, // _default
        );
        Self::new(
            db,
            typevar,
            BindingContext::Synthetic(env.program(db)),
            None,
            TypeVarNonce::NONE,
        )
    }

    /// Create a new synthetic `Self` type variable with the given upper bound.
    pub(crate) fn synthetic_self(
        db: &'db dyn Db,
        upper_bound: Type<'db>,
        binding_context: BindingContext<'db>,
    ) -> Self {
        let identity = TypeVarIdentity::new(
            db,
            Name::new_static("Self"),
            None, // definition
            TypeVarKind::TypingSelf,
        );
        let typevar = TypeVarInstance::new(
            db,
            identity,
            Some(TypeVarBoundOrConstraints::UpperBound(upper_bound).into()),
            Some(TypeVarVariance::Invariant),
            None, // _default
        );
        Self::new(db, typevar, binding_context, None, TypeVarNonce::NONE)
    }

    /// Applies a specialization to this occurrence's declared upper bound or constraints, if any.
    fn apply_specialization_to_bound_or_constraints(
        self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> Self {
        match retained_self::retain_self_domain_sync(
            self, specialization, env, &retained_self::OrdinaryRetainedSelf { db },
        ) {
            Ok(variable) => variable,
            Err(never) => match never {},
        }
    }

    /// Returns an identical type variable with its `TypeVarBoundOrConstraints` mapped by the
    /// provided closure.
    pub(crate) fn map_bound_or_constraints(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(Option<TypeVarBoundOrConstraints<'db>>) -> Option<TypeVarBoundOrConstraints<'db>>,
    ) -> Self {
        let env = ProgramEnvironment::from_program(self.binding_context(db).program(db));
        let typevar = self.typevar(db);
        let bound_or_constraints = f(typevar.bound_or_constraints(db, &env));
        let typevar = TypeVarInstance::new(
            db,
            typevar.identity(db),
            bound_or_constraints.map(TypeVarBoundOrConstraintsEvaluation::Eager),
            typevar.explicit_variance(db),
            typevar._default(db),
        );

        Self::new(
            db,
            typevar,
            self.binding_context(db),
            self.paramspec_attr(db),
            self.freshness(db),
        )
    }

    pub(crate) fn variance_with_polarity(
        self,
        db: &'db dyn Db,
        polarity: TypeVarVariance,
    ) -> TypeVarVariance {
        let _span = tracing::trace_span!("variance_with_polarity").entered();

        match self.typevar(db).explicit_variance(db) {
            Some(explicit_variance) => explicit_variance.compose(polarity),
            None => match self.binding_context(db) {
                BindingContext::Definition(definition) => polarity.compose_thunk(|| {
                    let env = ProgramEnvironment::from_definition(definition);
                    match binding_type(db, definition)
                        .variance_of(db, &env, self.identity(db))
                        .evaluate(db)
                    {
                        // When both directions are valid, the typing spec selects covariance.
                        TypeVarVariance::Bivariant => TypeVarVariance::Covariant,
                        variance => variance,
                    }
                }),
                BindingContext::Synthetic(_) => TypeVarVariance::Invariant,
            },
        }
    }

    pub fn variance(self, db: &'db dyn Db) -> TypeVarVariance {
        self.variance_with_polarity(db, TypeVarVariance::Covariant)
    }

    pub(super) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Type<'db> {
        inline_mapping_result(self.apply_type_mapping_sync(
            db,
            type_mapping,
            visitor,
            &InlineMappingEffects,
        ))
    }

    pub(super) fn bind_legacy_typevars(self) -> Type<'db> {
        Type::TypeVar(self)
    }

    #[ty_mapping_probe_macros::dual_mapping]
    pub(super) async fn apply_type_mapping_with<'a, E: MappingEffects<'db>>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        match self
            .mapping_start_with(db, type_mapping, visitor, effects)
            .await?
        {
            MappingStart::Complete(result) => Ok(result),
            MappingStart::Continue(continuation) => {
                continuation.resume_mapping_with(db, visitor, effects).await
            }
        }
    }

    #[ty_mapping_probe_macros::dual_mapping]
    pub(super) async fn mapping_start_with<'a, E: MappingStartEffects<'db>>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
        effects: &E,
    ) -> Result<MappingStart<Type<'db>, TypeVarMappingContinuation<'db>>, E::Error> {
        let specialize = |specialization: &ApplySpecialization<'a, 'db>| {
            inline_mapping_result(specialize_bound_typevar_sync(
                self,
                specialization,
                TypeVarSpecializationFacts,
                &OrdinaryTypeVarSpecialization { db },
            ))
        };

        let possibly_apply_to_self = |specialization: &ApplySpecialization<'a, 'db>| {
            if self.typevar(db).is_self(db)
                && specialization.specialize_self_domain()
                && let Some(specialization) = specialization.as_specialization(db)
            {
                Type::TypeVar(self.apply_specialization_to_bound_or_constraints(
                    db,
                    specialization,
                    visitor.env,
                ))
            } else {
                Type::TypeVar(self)
            }
        };

        Ok(MappingStart::Complete(match type_mapping {
            TypeMapping::ApplySpecialization(specialization) => {
                let result =
                    if !matches!(specialization, ApplySpecialization::Specialization { .. } | ApplySpecialization::ReturnCallables(_)) {
                        effects.legacy(MappingOperation::MappingMode, || {
                            specialize(specialization)
                        })?
                    } else if self.is_paramspec(db) || self.paramspec_attr(db).is_some() {
                        effects.legacy(MappingOperation::ParamSpec, || {
                            specialize(specialization)
                        })?
                    } else {
                        effects.checkpoint(MappingWork::TypeVarLookup).await?;
                        specialize(specialization)
                    };
                match result {
                    TypeVarSpecialization::Type(mapped) => mapped,
                    TypeVarSpecialization::RetainedSelf => {
                        effects.legacy(MappingOperation::RetainedSelf, || {
                            possibly_apply_to_self(specialization)
                        })?
                    }
                }
            }
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            } => effects.legacy(MappingOperation::MappingMode, || {
                match specialize(specialization) {
                    TypeVarSpecialization::Type(mapped) => {
                        // Only materialize if the specialization actually substituted this
                        // typevar with a different type. A typevar that maps back to itself
                        // hasn't been substituted and should not be materialized.
                        if mapped == Type::TypeVar(self) {
                            mapped
                        } else {
                            let env = visitor.env;
                            // Materialization uses a different mapping mode. Reuse of the outer
                            // visitor can incorrectly hit a cache entry from specialization.
                            let materialization_visitor = visitor.for_new_materialization_root();
                            let materialized = mapped.materialize(
                                db,
                                *materialization_kind,
                                &materialization_visitor,
                            );

                            if *materialization_kind == MaterializationKind::Top
                                && !materialization_visitor.is_equivalent_to_materialization(
                                    db,
                                    mapped,
                                    materialized,
                                )
                                && let Some(upper_bound) = self.top_materialized_upper_bound(db)
                            {
                                IntersectionType::from_two_elements(
                                    db,
                                    env,
                                    materialized,
                                    upper_bound,
                                )
                            } else {
                                materialized
                            }
                        }
                    }
                    TypeVarSpecialization::RetainedSelf => possibly_apply_to_self(specialization),
                }
            })?,
            TypeMapping::BindSelf(binding) => {
                return Ok(MappingStart::Continue(TypeVarMappingContinuation {
                    binding: *binding,
                    variable: self,
                }));
            }
            TypeMapping::ReplaceSelf { new_upper_bound } => {
                effects.legacy(MappingOperation::MappingMode, || {
                    if self.typevar(db).is_self(db) {
                        Type::TypeVar(BoundTypeVarInstance::synthetic_self(
                            db,
                            *new_upper_bound,
                            self.binding_context(db),
                        ))
                    } else {
                        Type::TypeVar(self)
                    }
                })?
            }
            TypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            } => effects.legacy(MappingOperation::MappingMode, || {
                Type::TypeVar(freshening::freshen_bound_typevar(
                    db,
                    self,
                    *generic_context,
                    *delta,
                    type_mapping,
                    visitor,
                ))
            })?,
            TypeMapping::Promote(..) | TypeMapping::RescopeReturnCallables(_) => Type::TypeVar(self),
            TypeMapping::BindLegacyTypevars(_) => self.bind_legacy_typevars(),
            TypeMapping::ReplaceParameterDefaults
            | TypeMapping::EagerExpansion
            | TypeMapping::ApplyRecursiveSubstitution(_) => {
                effects.legacy(MappingOperation::MappingMode, || Type::TypeVar(self))?
            }
            TypeMapping::Materialize(materialization_kind) => {
                effects.legacy(MappingOperation::MappingMode, || {
                    if visitor.materialize_typevar_bounds_and_defaults {
                        Type::TypeVar(self.materialize_impl(db, *materialization_kind, visitor))
                    } else {
                        Type::TypeVar(self)
                    }
                })?
            }
        }))
    }

    /// Returns the static upper bound used when materializing a gradual type argument.
    ///
    /// Constraints are unioned only when materializing an exposed member, where their union is a
    /// valid conservative upper bound. A bound may recursively refer to its own generic class,
    /// either directly or through other bounds. Such a bound has no finite static top
    /// materialization, so recover from its cycle without applying an upper bound.
    pub(super) fn top_materialized_upper_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        top_materialized_upper_bound_inner(db, self)
    }
}

pub(super) fn walk_bound_type_var_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    bound_typevar: BoundTypeVarInstance<'db>,
    visitor: &V,
) {
    visitor.visit_type_var_type(db, bound_typevar.typevar(db));
}

impl<'db> BoundTypeVarInstance<'db> {
    /// Returns the default value of this typevar, recursively applying its binding context to any
    /// other typevars that appear in the default.
    ///
    /// For instance, in
    ///
    /// ```py
    /// T = TypeVar("T")
    /// U = TypeVar("U", default=T)
    ///
    /// # revealed: typing.TypeVar[U = typing.TypeVar[T]]
    /// reveal_type(U)
    ///
    /// # revealed: typing.Generic[T, U = T@C]
    /// class C(reveal_type(Generic[T, U])): ...
    /// ```
    ///
    /// In the first case, the use of `U` is unbound, and so we have a [`TypeVarInstance`], and its
    /// default value (`T`) is also unbound.
    ///
    /// By using `U` in the generic class, it becomes bound, and so we have a
    /// `BoundTypeVarInstance`. As part of binding `U` we must also bind its default value
    /// (resulting in `T@C`).
    pub(crate) fn default_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        bound_typevar_default_type(db, self)
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        Self::new(
            db,
            self.typevar(db)
                .materialize_impl(db, materialization_kind, visitor),
            self.binding_context(db),
            self.paramspec_attr(db),
            self.freshness(db),
        )
    }

    pub(super) fn to_instance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<InstanceProjection<Self>> {
        Some(self.typevar(db).to_instance(db, env)?.map(|typevar| {
            Self::new(
                db,
                typevar,
                self.binding_context(db),
                self.paramspec_attr(db),
                self.freshness(db),
            )
        }))
    }
}

/// The kind of element that can be assigned to a typevar.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub(crate) enum TypeVarDomain {
    /// "Plain" typevars and `ParamSpec` components (e.g. `P.args` and `P.kwargs`) are mapped to
    /// types
    Type,
    /// `ParamSpec`s are mapped to parameter signatures
    ParameterSignature,
    /// `TypeVarTuple`s are mapped to type tuples
    TypeTuple,
}

impl TypeVarDomain {
    pub(crate) fn bottom(self, db: &dyn Db) -> Type<'_> {
        match self {
            TypeVarDomain::Type => Type::Never,
            TypeVarDomain::ParameterSignature => {
                Type::paramspec_value_callable(db, Parameters::bottom())
            }
            // TODO: Choose the correct top type once we support TypeVarTuple in constraint sets
            TypeVarDomain::TypeTuple => Type::Never,
        }
    }

    pub(crate) fn top(self, db: &dyn Db) -> Type<'_> {
        match self {
            TypeVarDomain::Type => Type::object(),
            TypeVarDomain::ParameterSignature => {
                Type::paramspec_value_callable(db, Parameters::top())
            }
            // TODO: Choose the correct top type once we support TypeVarTuple in constraint sets
            TypeVarDomain::TypeTuple => Type::object(),
        }
    }
}

/// Whether this typevar was created via the legacy `TypeVar` constructor, using PEP 695 syntax,
/// or an implicit typevar like `Self` was used.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum TypeVarKind {
    /// `T = TypeVar("T")`
    LegacyTypeVar,
    /// `def foo[T](x: T) -> T: ...`
    Pep695TypeVar,
    /// `typing.Self`
    TypingSelf,
    /// `P = ParamSpec("P")`
    LegacyParamSpec,
    /// `def foo[**P]() -> None: ...`
    Pep695ParamSpec,
    /// `Ts = TypeVarTuple("Ts")`
    LegacyTypeVarTuple,
    /// `def foo[*Ts]() -> None: ...`
    Pep695TypeVarTuple,
    /// `Alias: typing.TypeAlias = T`
    Pep613Alias,
}

impl TypeVarKind {
    pub(super) const fn is_pep695(self) -> bool {
        match self {
            Self::Pep695TypeVar | Self::Pep695ParamSpec | Self::Pep695TypeVarTuple => true,
            Self::LegacyTypeVar
            | Self::TypingSelf
            | Self::LegacyParamSpec
            | Self::LegacyTypeVarTuple
            | Self::Pep613Alias => false,
        }
    }

    pub(super) const fn is_paramspec(self) -> bool {
        matches!(self, Self::LegacyParamSpec | Self::Pep695ParamSpec)
    }

    pub(super) const fn is_typevartuple(self) -> bool {
        matches!(self, Self::LegacyTypeVarTuple | Self::Pep695TypeVarTuple)
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) LazyBoundUncheckedConfiguration), self_ty = TypeVarInstance<'db>, attempt = ReturnOnly,
    returns(copy),
    cycle_fn=lazy_bound_cycle_recover,
    cycle_initial=|_, _, _| None,
    heap_size=ruff_memory_usage::heap_size
)]
fn lazy_bound_unchecked<'db>(db: &'db dyn Db, this: TypeVarInstance<'db>) -> Option<Type<'db>> {
    let definition = this.definition(db)?;
    let program_file = definition.program_file(db);
    let python_file = program_file.python_file(db);
    let module = parsed_module(db, python_file).load(db);
    let ty = match definition.kind(db) {
        // PEP 695 typevar
        DefinitionKind::TypeVar(typevar) => {
            let typevar_node = typevar.node(&module);
            definition_expression_type(db, definition, typevar_node.bound.as_ref()?)
        }
        // legacy typevar
        DefinitionKind::Assignment(assignment) => {
            let call_expr = assignment.value(&module).as_call_expr()?;
            let expr = &call_expr.arguments.find_keyword("bound")?.value;
            definition_expression_type(db, definition, expr)
        }
        _ => return None,
    };

    Some(ty)
}

#[salsa::tracked(configuration = (pub(in crate::types) LazyConstraintsUncheckedConfiguration), self_ty = TypeVarInstance<'db>, attempt = ReturnOnly,
    returns(copy),
    cycle_fn=lazy_constraints_cycle_recover,
    cycle_initial=|_, _, _| None,
    heap_size=ruff_memory_usage::heap_size
)]
fn lazy_constraints_unchecked<'db>(
    db: &'db dyn Db,
    this: TypeVarInstance<'db>,
) -> Option<TypeVarConstraints<'db>> {
    let definition = this.definition(db)?;
    let program_file = definition.program_file(db);
    let python_file = program_file.python_file(db);
    let env = ProgramEnvironment::from_file(program_file);
    let module = parsed_module(db, python_file).load(db);
    let constraints = match definition.kind(db) {
        // PEP 695 typevar
        DefinitionKind::TypeVar(typevar) => {
            let typevar_node = typevar.node(&module);
            let bound = definition_expression_type(db, definition, typevar_node.bound.as_ref()?);
            if let Some(tuple) = bound.tuple_instance_spec(db, &env)
                && let Tuple::Fixed(tuple) = tuple.into_owned()
            {
                TypeVarConstraints::new(db, tuple.owned_elements())
            } else {
                TypeVarConstraints::new(db, [Type::unknown()].as_slice())
            }
        }
        // legacy typevar
        DefinitionKind::Assignment(assignment) => {
            let call_expr = assignment.value(&module).as_call_expr()?;
            TypeVarConstraints::new(
                db,
                call_expr
                    .arguments
                    .args
                    .iter()
                    .skip(1)
                    .map(|arg| definition_expression_type(db, definition, arg))
                    .collect::<Box<_>>(),
            )
        }
        _ => return None,
    };

    Some(constraints)
}

#[salsa::tracked(configuration = (pub(in crate::types) LazyDefaultUncheckedConfiguration), self_ty = TypeVarInstance<'db>, attempt = ReturnOnly, returns(copy), cycle_initial=|_, id, _| Some(Type::divergent(id)), cycle_fn=lazy_default_cycle_recover, heap_size=ruff_memory_usage::heap_size)]
fn lazy_default_unchecked<'db>(db: &'db dyn Db, this: TypeVarInstance<'db>) -> Option<Type<'db>> {
    let Ok(default) = default::lazy::lazy_default_sync(
        this,
        default::lazy::LazyDefaultFacts,
        &default::lazy::OrdinaryLazyDefaultEffects { db },
    );
    default
}

#[salsa::tracked(configuration = (pub(in crate::types) TopMaterializedUpperBoundInnerConfiguration),
    attempt = ReturnOnly,
    returns(copy),
    cycle_result=|_, _, _| None,
    heap_size=ruff_memory_usage::heap_size
)]
fn top_materialized_upper_bound_inner<'db>(
    db: &'db dyn Db,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<Type<'db>> {
    let env = ProgramEnvironment::from_program(bound_typevar.binding_context(db).program(db));

    bound_typevar
        .typevar(db)
        .bound_or_constraints(db, &env)
        .map(|bound_or_constraints| {
            bound_or_constraints
                .as_type(db, &env)
                .top_materialization(db, &env)
        })
}

/// The identity of a type variable.
///
/// This represents the core identity of a typevar, independent of its bounds or constraints. Two
/// typevars have the same identity if they represent the same logical typevar, even if their
/// bounds have been materialized differently.
#[salsa::interned(debug, field_view = read_fields, field_requests = field_requests, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeVarIdentity<'db> {
    /// The name of this TypeVar (e.g. `T`)
    #[returns(ref)]
    pub(crate) name: Name,

    /// The type var's definition (None if synthesized)
    #[returns(copy)]
    pub(crate) definition: Option<Definition<'db>>,

    /// The kind of typevar (PEP 695, Legacy, or TypingSelf)
    #[returns(copy)]
    pub(crate) kind: TypeVarKind,
}

impl get_size2::GetSize for TypeVarIdentity<'_> {}

#[expect(clippy::ref_option)]
fn lazy_bound_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &Option<Type<'db>>,
    current: Option<Type<'db>>,
    typevar: TypeVarInstance<'db>,
) -> Option<Type<'db>> {
    // Normalize the bounds/constraints to ensure cycle convergence.
    let current = current?;
    let program_file = typevar
        .definition(db)
        .expect("a lazy TypeVar bound must have a source definition")
        .program_file(db);
    let env = ProgramEnvironment::from_file(program_file);
    Some(match previous {
        Some(prev) => current.cycle_normalized(db, &env, *prev, cycle),
        None => current.recursive_type_normalized(db, &env, cycle),
    })
}

#[allow(clippy::trivially_copy_pass_by_ref)]
#[expect(clippy::ref_option)]
fn lazy_constraints_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &Option<TypeVarConstraints<'db>>,
    current: Option<TypeVarConstraints<'db>>,
    typevar: TypeVarInstance<'db>,
) -> Option<TypeVarConstraints<'db>> {
    // Normalize the bounds/constraints to ensure cycle convergence.
    let current = current?;
    let program_file = typevar
        .definition(db)
        .expect("lazy TypeVar constraints must have a source definition")
        .program_file(db);
    let env = ProgramEnvironment::from_file(program_file);
    Some(match previous {
        Some(prev) => current.cycle_normalized(db, &env, *prev, cycle),
        None => current.recursive_type_normalized(db, &env, cycle),
    })
}

#[expect(clippy::ref_option)]
fn lazy_default_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_default: &Option<Type<'db>>,
    current: Option<Type<'db>>,
    typevar: TypeVarInstance<'db>,
) -> Option<Type<'db>> {
    let Ok(default) = default::lazy::lazy_default_recover_sync(
        cycle,
        *previous_default,
        current,
        typevar,
        default::lazy::LazyDefaultFacts,
        &default::lazy::OrdinaryLazyDefaultEffects { db },
    );
    default
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn lazy_typevar_default_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<LazyDefaultUncheckedConfiguration> {
    lazy_default_unchecked::fn_ingredient_(db, db.zalsa())
}

/// Where a type variable is bound and usable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum BindingContext<'db> {
    /// The definition of the generic class, function, or type alias that binds this typevar.
    Definition(Definition<'db>),
    /// The typevar is synthesized internally, and is not associated with a particular definition
    /// in the source, but is still bound and eligible for specialization inference. Its program
    /// identifies the environment that cannot otherwise be recovered from a source definition.
    Synthetic(Program<'db>),
}

impl<'db> From<Definition<'db>> for BindingContext<'db> {
    fn from(definition: Definition<'db>) -> Self {
        BindingContext::Definition(definition)
    }
}

impl<'db> BindingContext<'db> {
    pub(crate) fn definition(self) -> Option<Definition<'db>> {
        match self {
            BindingContext::Definition(definition) => Some(definition),
            BindingContext::Synthetic(_) => None,
        }
    }

    fn program(self, db: &'db dyn Db) -> Program<'db> {
        match self {
            Self::Definition(definition) => definition.program(db),
            Self::Synthetic(program) => program,
        }
    }

    pub(super) fn name(self, db: &'db dyn Db) -> Option<String> {
        self.definition().and_then(|definition| definition.name(db))
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, get_size2::GetSize)]
pub(crate) enum ParamSpecAttrKind {
    Args,
    Kwargs,
}

impl ParamSpecAttrKind {
    /// Returns the component represented by a `ParamSpec` attribute name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "args" => Some(Self::Args),
            "kwargs" => Some(Self::Kwargs),
            _ => None,
        }
    }
}

impl std::fmt::Display for ParamSpecAttrKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamSpecAttrKind::Args => f.write_str("args"),
            ParamSpecAttrKind::Kwargs => f.write_str("kwargs"),
        }
    }
}

/// The identity of a bound type variable occurrence.
///
/// This identifies a specific binding of a typevar to a context (e.g., `T@ClassC` vs `T@FunctionF`),
/// plus a freshness nonce for fresh callable occurrences, independent of the typevar's
/// bounds or constraints. Two bound typevars have the same identity if they represent the same
/// occurrence, even if their bounds have been materialized differently. Two fresh occurrences of
/// the same source-level typevar have different bound identities.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct BoundTypeVarIdentity<'db> {
    pub(crate) identity: TypeVarIdentity<'db>,
    pub(crate) binding_context: BindingContext<'db>,
    /// If [`Some`], this indicates that this type variable is the `args` or `kwargs` component
    /// of a `ParamSpec` i.e., `P.args` or `P.kwargs`.
    pub(super) paramspec_attr: Option<ParamSpecAttrKind>,
    /// The freshness nonce for this bound typevar occurrence; `0` is the source-level occurrence.
    freshness: TypeVarNonce,
}

impl<'db> BoundTypeVarIdentity<'db> {
    pub(in crate::types) fn new(
        identity: TypeVarIdentity<'db>,
        binding_context: BindingContext<'db>,
        paramspec_attr: Option<ParamSpecAttrKind>,
        freshness: TypeVarNonce,
    ) -> Self {
        Self {
            identity,
            binding_context,
            paramspec_attr,
            freshness,
        }
    }

    fn kind(self, db: &'db dyn Db) -> TypeVarKind {
        self.kind_with_fields(salsa::FieldReads::new(db))
    }

    fn kind_with_fields(self, fields: salsa::FieldReads<'db>) -> TypeVarKind {
        *self.identity.read_fields(fields).kind()
    }

    pub(crate) fn is_paramspec(self, db: &'db dyn Db) -> bool {
        self.kind(db).is_paramspec()
    }

    pub(crate) fn without_paramspec_attr(mut self, db: &'db dyn Db) -> Self {
        debug_assert!(
            self.is_paramspec(db),
            "Expected a ParamSpec, got {:?}",
            self.kind(db)
        );

        self.paramspec_attr = None;
        self
    }
}

/// A set of bound typevar occurrences.
///
/// Membership is keyed by [`BoundTypeVarIdentity`], including any freshness nonce, while the first
/// bound instance encountered for each identity is retained. This lets a fresh generic-callable
/// occurrence be inferable without making the surrounding source-level typevar inferable.
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum TypeVarSet<'db> {
    None,
    Some(TypeVarSetInner<'db>),
}

impl<'db> TypeVarSet<'db> {
    pub(crate) fn from_typevars(
        db: &'db dyn Db,
        typevars: impl IntoIterator<Item = BoundTypeVarInstance<'db>>,
    ) -> Self {
        construction::from_typevars(db, typevars)
    }
}

#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct TypeVarSetInner<'db> {
    #[returns(ref)]
    typevars: FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeVarSetInner<'_> {}

impl<'db> BoundTypeVarIdentity<'db> {
    pub(crate) fn is_inferable(self, db: &'db dyn Db, inferable: TypeVarSet<'db>) -> bool {
        match inferable {
            TypeVarSet::None => false,
            TypeVarSet::Some(inner) => inner.typevars(db).contains_key(&self),
        }
    }
}

impl<'db> BoundTypeVarInstance<'db> {
    pub(crate) fn is_inferable(self, db: &'db dyn Db, inferable: TypeVarSet<'db>) -> bool {
        self.identity(db).is_inferable(db, inferable)
    }
}

impl<'db> TypeVarSet<'db> {
    pub(crate) fn merge(self, db: &'db dyn Db, other: Self) -> Self {
        #[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
        fn merge_inner<'db>(
            db: &'db dyn Db,
            self_inner: TypeVarSetInner<'db>,
            other_inner: TypeVarSetInner<'db>,
        ) -> TypeVarSet<'db> {
            TypeVarSet::from_typevars(
                db,
                self_inner
                    .typevars(db)
                    .values()
                    .chain(other_inner.typevars(db).values())
                    .copied(),
            )
        }

        match (self, other) {
            (TypeVarSet::None, other) | (other, TypeVarSet::None) => other,
            (TypeVarSet::Some(self_inner), TypeVarSet::Some(other_inner)) => {
                merge_inner(db, self_inner, other_inner)
            }
        }
    }

    // This is not an IntoIterator implementation because I have no desire to try to name the
    // iterator type.
    pub(crate) fn iter(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = BoundTypeVarInstance<'db>> + 'db {
        match self {
            TypeVarSet::None => Either::Left(std::iter::empty()),
            TypeVarSet::Some(inner) => Either::Right(inner.typevars(db).values().copied()),
        }
    }

    // Keep this around for debugging purposes
    #[cfg_attr(not(test), expect(dead_code))]
    fn display(self, db: &'db dyn Db) -> String {
        format!(
            "[{}]",
            self.iter(db)
                .map(|typevar| typevar.identity(db).display(db))
                .format(", ")
        )
    }
}

#[salsa::tracked(configuration = (pub(in crate::types) BoundTypeVarDefaultTypeConfiguration), attempt = ReturnOnly,
    returns(copy),
    cycle_initial=|_, id, _| Some(Type::divergent(id)),
    cycle_fn=bound_typevar_default_type_cycle_recover,
    heap_size=ruff_memory_usage::heap_size
)]
fn bound_typevar_default_type<'db>(
    db: &'db dyn Db,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<Type<'db>> {
    let Ok(default) = default::bound_typevar_default_sync(
        bound_typevar,
        default::BoundDefaultFacts,
        &default::OrdinaryBoundDefaultEffects { db },
    );
    default
}

#[expect(clippy::ref_option)]
fn bound_typevar_default_type_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_default: &Option<Type<'db>>,
    default: Option<Type<'db>>,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<Type<'db>> {
    let Ok(default) = default::bound_typevar_default_recover_sync(
        cycle,
        *previous_default,
        default,
        bound_typevar,
        default::BoundDefaultFacts,
        &default::OrdinaryBoundDefaultEffects { db },
    );
    default
}

#[cfg(any(test, feature = "experimental-analysis"))]
pub(in crate::types) fn bound_typevar_default_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<BoundTypeVarDefaultTypeConfiguration> {
    bound_typevar_default_type::fn_ingredient_(db, db.zalsa())
}

/// Whether a typevar default is eagerly specified or lazily evaluated.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub enum TypeVarDefaultEvaluation<'db> {
    /// The default type is lazily evaluated.
    Lazy,
    /// The default type is eagerly specified.
    Eager(Type<'db>),
}

impl<'db> From<Type<'db>> for TypeVarDefaultEvaluation<'db> {
    fn from(value: Type<'db>) -> Self {
        TypeVarDefaultEvaluation::Eager(value)
    }
}

pub(in crate::types) fn decode_eager_default<'db>(
    evaluation: Option<TypeVarDefaultEvaluation<'db>>,
) -> (Option<Type<'db>>, bool) {
    match evaluation {
        Some(TypeVarDefaultEvaluation::Eager(default_type)) => (Some(default_type), false),
        Some(TypeVarDefaultEvaluation::Lazy) => (None, true),
        None => (None, false),
    }
}

/// Whether a typevar bound/constraints is eagerly specified or lazily evaluated.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub enum TypeVarBoundOrConstraintsEvaluation<'db> {
    /// There is a lazily-evaluated upper bound.
    LazyUpperBound,
    /// There is a lazily-evaluated set of constraints.
    LazyConstraints,
    /// The upper bound/constraints are eagerly specified.
    Eager(TypeVarBoundOrConstraints<'db>),
}

impl<'db> From<TypeVarBoundOrConstraints<'db>> for TypeVarBoundOrConstraintsEvaluation<'db> {
    fn from(value: TypeVarBoundOrConstraints<'db>) -> Self {
        TypeVarBoundOrConstraintsEvaluation::Eager(value)
    }
}

pub(in crate::types) fn decode_eager_bounds<'db>(
    evaluation: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
) -> (Option<TypeVarBoundOrConstraints<'db>>, bool) {
    match evaluation {
        Some(TypeVarBoundOrConstraintsEvaluation::Eager(bounds)) => (Some(bounds), false),
        Some(
            TypeVarBoundOrConstraintsEvaluation::LazyUpperBound
            | TypeVarBoundOrConstraintsEvaluation::LazyConstraints,
        ) => (None, true),
        None => (None, false),
    }
}

/// Type variable constraints (e.g. `T: (int, str)`).
/// This is structurally identical to [`UnionType`], except that it does not perform simplification and preserves the element types.
#[salsa::interned(field_view = read_fields, field_requests = field_requests, debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeVarConstraints<'db> {
    #[returns(ref)]
    pub(super) elements: Box<[Type<'db>]>,
}

impl get_size2::GetSize for TypeVarConstraints<'_> {}

fn walk_type_var_constraints<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    constraints: TypeVarConstraints<'db>,
    visitor: &V,
) {
    for ty in constraints.elements(db) {
        visitor.visit_type(db, *ty);
    }
}

impl<'db> TypeVarConstraints<'db> {
    pub(super) fn as_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        UnionType::from_elements(db, env, self.elements(db))
    }

    fn to_instance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<InstanceProjection<TypeVarConstraints<'db>>> {
        let mut instance_elements = Vec::new();
        let mut is_exact = true;
        for ty in self.elements(db) {
            let projection = ty.to_instance(db, env)?;
            is_exact &= projection.is_exact();
            instance_elements.push(projection.into_inner());
        }
        Some(InstanceProjection::new(
            TypeVarConstraints::new(db, instance_elements.into_boxed_slice()),
            is_exact,
        ))
    }

    pub(super) fn map(
        self,
        db: &'db dyn Db,
        transform_fn: impl FnMut(&Type<'db>) -> Type<'db>,
    ) -> Self {
        let mapped = self
            .elements(db)
            .iter()
            .map(transform_fn)
            .collect::<Box<_>>();
        TypeVarConstraints::new(db, mapped)
    }

    pub(crate) fn map_with_boundness_and_qualifiers(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let mut builder = UnionBuilder::new(db, env);
        let mut qualifiers = TypeQualifiers::empty();

        let mut all_unbound = true;
        let mut possibly_unbound = false;
        let mut origin = TypeOrigin::Declared;
        for ty in self.elements(db) {
            let PlaceAndQualifiers {
                place: ty_member,
                qualifiers: new_qualifiers,
            } = transform_fn(ty);
            qualifiers |= new_qualifiers;
            match ty_member {
                Place::Undefined => {
                    possibly_unbound = true;
                }
                Place::Defined(DefinedPlace {
                    ty: ty_member,
                    origin: member_origin,
                    definedness: member_boundness,
                    ..
                }) => {
                    origin = origin.merge(member_origin);
                    if member_boundness == Definedness::PossiblyUndefined {
                        possibly_unbound = true;
                    }

                    all_unbound = false;
                    builder = builder.add(ty_member);
                }
            }
        }
        PlaceAndQualifiers {
            place: if all_unbound {
                Place::Undefined
            } else {
                Place::Defined(DefinedPlace {
                    ty: builder.build(),
                    origin,
                    definedness: if possibly_unbound {
                        Definedness::PossiblyUndefined
                    } else {
                        Definedness::AlwaysDefined
                    },
                    public_type_policy: PublicTypePolicy::Raw,
                    provenance: Provenance::Unknown,
                })
            },
            qualifiers,
        }
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let materialized = self
            .elements(db)
            .iter()
            .map(|ty| ty.materialize(db, materialization_kind, visitor))
            .collect::<Box<_>>();
        TypeVarConstraints::new(db, materialized)
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let mapped = self
            .elements(db)
            .iter()
            .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, TypeContext::default(), visitor))
            .collect::<Box<_>>();
        TypeVarConstraints::new(db, mapped)
    }

    /// Normalize for cycle recovery by combining with the previous value and
    /// removing divergent types introduced by the cycle.
    ///
    /// See [`Type::cycle_normalized`] for more details on how this works.
    fn cycle_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        previous: Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        let current_elements = self.elements(db);
        let prev_elements = previous.elements(db);
        TypeVarConstraints::new(
            db,
            current_elements
                .iter()
                .zip(prev_elements.iter())
                .map(|(ty, prev_ty)| ty.cycle_normalized(db, env, *prev_ty, cycle))
                .collect::<Box<_>>(),
        )
    }

    /// Normalize recursive types for cycle recovery when there's no previous value.
    ///
    /// See [`Type::recursive_type_normalized`] for more details.
    fn recursive_type_normalized(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        cycle: &salsa::Cycle,
    ) -> Self {
        self.map(db, |ty| ty.recursive_type_normalized(db, env, cycle))
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, get_size2::GetSize, salsa::SalsaValue)]
pub enum TypeVarBoundOrConstraints<'db> {
    UpperBound(Type<'db>),
    Constraints(TypeVarConstraints<'db>),
}

fn walk_type_var_bounds<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    bounds: TypeVarBoundOrConstraints<'db>,
    visitor: &V,
) {
    match bounds {
        TypeVarBoundOrConstraints::UpperBound(bound) => {
            visitor.visit_type(db, bound);
        }
        TypeVarBoundOrConstraints::Constraints(constraints) => {
            walk_type_var_constraints(db, constraints, visitor);
        }
    }
}

impl<'db> TypeVarBoundOrConstraints<'db> {
    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => TypeVarBoundOrConstraints::UpperBound(
                bound.materialize(db, materialization_kind, visitor),
            ),
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(constraints.materialize_impl(
                    db,
                    materialization_kind,
                    visitor,
                ))
            }
        }
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => TypeVarBoundOrConstraints::UpperBound(
                bound.apply_type_mapping_impl(db, type_mapping, TypeContext::default(), visitor),
            ),
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(constraints.apply_type_mapping_impl(
                    db,
                    type_mapping,
                    visitor,
                ))
            }
        }
    }

    /// Represent the bound/constraints of this typevar as a single type, by unioning constraints.
    ///
    /// Careful with this method! It has both semantic and performance gotchas. Unioning
    /// constraints provides a conservative upper bound, but it loses precision. And for many use
    /// cases, it's more efficient to just map over the constraint types directly, rather than
    /// building a union out of them and mapping over that.
    pub(crate) fn as_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => bound,
            TypeVarBoundOrConstraints::Constraints(constraints) => constraints.as_type(db, env),
        }
    }
}

/// A [`CycleDetector`] that is used in `TypeVarInstance::default_type`.
pub(crate) type TypeVarDefaultVisitor<'db> =
    CycleDetector<'db, VisitTypeVarDefault, TypeVarInstance<'db>, Option<Type<'db>>, 6>;
pub(crate) struct VisitTypeVarDefault;

impl<'db> super::cyclic::HasIdentity<'db> for TypeVarInstance<'db> {
    type Id = Self;

    const CYCLE_IDENTITY_MODE: CycleIdentityMode = CycleIdentityMode::Exact;

    fn to_identity(&self, _db: &'db dyn Db) -> Self::Id {
        *self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ruff_db::testing::assert_function_query_was_not_run_by_name;

    use crate::db::tests::setup_db;

    fn bound_typevar<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &'static str,
        kind: TypeVarKind,
        bound_or_constraints: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        freshness: TypeVarNonce,
    ) -> BoundTypeVarInstance<'db> {
        let identity = TypeVarIdentity::new(db, Name::new_static(name), None, kind);
        let typevar = TypeVarInstance::new(
            db,
            identity,
            bound_or_constraints,
            Some(TypeVarVariance::Invariant),
            None,
        );
        BoundTypeVarInstance::new(
            db,
            typevar,
            BindingContext::Synthetic(env.program(db)),
            None,
            freshness,
        )
    }

    #[test]
    fn typevar_set_empty_set_is_none() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let typevar = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let inferable = TypeVarSet::from_typevars(db, []);

        assert_eq!(inferable, TypeVarSet::None);
        assert_eq!(inferable.iter(db).count(), 0);
        assert!(!typevar.is_inferable(db, inferable));
        assert!(!typevar.identity(db).is_inferable(db, inferable));
    }

    #[test]
    fn typevar_set_keeps_first_instance_for_each_identity() {
        let mut db = setup_db();
        db.clear_salsa_events();
        let env = db.program_environment();

        // The synthetic lazy bound has no definition, so it is equivalent to the implicit
        // `object` upper bound represented eagerly below.
        let lazy = bound_typevar(
            &db,
            &env,
            "T",
            TypeVarKind::Pep695TypeVar,
            Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            TypeVarNonce::NONE,
        );
        let eager = bound_typevar(
            &db,
            &env,
            "T",
            TypeVarKind::Pep695TypeVar,
            Some(TypeVarBoundOrConstraints::UpperBound(Type::object()).into()),
            TypeVarNonce::NONE,
        );
        let u = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let v = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("V"),
            TypeVarVariance::Invariant,
        );

        assert_ne!(lazy, eager);
        assert_eq!(lazy.identity(&db), eager.identity(&db));

        let left = TypeVarSet::from_typevars(&db, [lazy, u, eager]);
        let right = TypeVarSet::from_typevars(&db, [eager, v, lazy]);
        let merged = left.merge(&db, right);

        assert_eq!(left.iter(&db).collect::<Vec<_>>(), [lazy, u]);
        assert_eq!(right.iter(&db).collect::<Vec<_>>(), [eager, v]);
        assert_eq!(merged.iter(&db).collect::<Vec<_>>(), [lazy, u, v]);
        assert_eq!(merged, TypeVarSet::from_typevars(&db, [lazy, u, v]));
        assert!(lazy.is_inferable(&db, merged));
        assert!(eager.is_inferable(&db, merged));
        assert_eq!(merged.display(&db), "[T, U, V]");

        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "lazy_bound_unchecked", None, &events);
    }

    #[test]
    fn typevar_set_distinguishes_fresh_and_paramspec_identities() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let typevar = bound_typevar(
            db,
            &env,
            "T",
            TypeVarKind::Pep695TypeVar,
            None,
            TypeVarNonce::NONE,
        );
        let fresh = bound_typevar(
            db,
            &env,
            "T",
            TypeVarKind::Pep695TypeVar,
            None,
            TypeVarNonce::NONE.increment(),
        );
        let paramspec = bound_typevar(
            db,
            &env,
            "P",
            TypeVarKind::Pep695ParamSpec,
            None,
            TypeVarNonce::NONE,
        );
        let args = paramspec.with_paramspec_attr(db, ParamSpecAttrKind::Args);
        let kwargs = paramspec.with_paramspec_attr(db, ParamSpecAttrKind::Kwargs);

        let inferable = TypeVarSet::from_typevars(db, [typevar, fresh, args, kwargs]);
        assert_eq!(
            inferable.iter(db).collect::<Vec<_>>(),
            [typevar, fresh, args, kwargs]
        );
        assert!(typevar.is_inferable(db, inferable));
        assert!(fresh.is_inferable(db, inferable));
        assert!(args.is_inferable(db, inferable));
        assert!(kwargs.is_inferable(db, inferable));
        assert!(!paramspec.is_inferable(db, inferable));

        let paramspec_only = TypeVarSet::from_typevars(db, [paramspec]);
        assert!(
            args.identity(db)
                .without_paramspec_attr(db)
                .is_inferable(db, paramspec_only)
        );
        assert!(
            kwargs
                .identity(db)
                .without_paramspec_attr(db)
                .is_inferable(db, paramspec_only)
        );
    }
}
