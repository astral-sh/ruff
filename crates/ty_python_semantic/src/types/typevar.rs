use std::cell::{Cell, RefCell};
use std::num::NonZeroU32;
use std::rc::Rc;

use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;

use crate::{
    Db, TypeQualifiers,
    place::{DefinedPlace, Definedness, Place, PlaceAndQualifiers, PublicTypePolicy, TypeOrigin},
    types::{
        ApplySpecialization, ApplyTypeMappingVisitor, CycleDetector, DynamicType, GenericContext,
        KnownClass, KnownInstanceType, MaterializationKind, Parameter, Parameters, Type,
        TypeAliasType, TypeContext, TypeMapping, TypeVarVariance, UnionBuilder, UnionType,
        any_over_type, binding_type, definition_expression_type, tuple::Tuple,
        variance::VarianceInferable, visitor,
    },
};
use ty_python_core::{
    definition::{Definition, DefinitionKind},
    semantic_index,
};

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

    pub(crate) fn has_typevar(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self, false, |ty| matches!(ty, Type::TypeVar(_)))
    }

    pub(crate) fn references_typevar(
        self,
        db: &'db dyn Db,
        typevar_id: TypeVarIdentity<'db>,
    ) -> bool {
        any_over_type(db, self, false, |ty| match ty {
            Type::TypeVar(bound_typevar) => typevar_id == bound_typevar.typevar(db).identity(db),
            Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => {
                typevar_id == typevar.identity(db)
            }
            _ => false,
        })
    }

    pub(crate) fn has_non_self_typevar(self, db: &'db dyn Db) -> bool {
        any_over_type(
            db,
            self,
            false,
            |ty| matches!(ty, Type::TypeVar(tv) if !tv.typevar(db).is_self(db)),
        )
    }

    pub(crate) fn has_typevar_or_typevar_instance(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self, false, |ty| {
            matches!(
                ty,
                Type::KnownInstance(KnownInstanceType::TypeVar(_)) | Type::TypeVar(_)
            )
        })
    }

    pub(crate) fn has_unspecialized_type_var(self, db: &'db dyn Db) -> bool {
        any_over_type(db, self, false, |ty| {
            matches!(ty, Type::Dynamic(DynamicType::UnspecializedTypeVar))
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeVarInstance<'db> {
    /// The identity of this typevar
    pub(crate) identity: TypeVarIdentity<'db>,

    /// The upper bound or constraint on the type of this TypeVar, if any. Don't use this field
    /// directly; use the `bound_or_constraints` (or `upper_bound` and `constraints`) methods
    /// instead (to evaluate any lazy bound or constraints).
    _bound_or_constraints: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,

    /// The explicitly specified variance of the TypeVar
    pub(super) explicit_variance: Option<TypeVarVariance>,

    /// The default type for this TypeVar, if any. Don't use this field directly, use the
    /// `default_type` method instead (to evaluate any lazy default).
    _default: Option<TypeVarDefaultEvaluation<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeVarInstance<'_> {}

pub(super) fn walk_type_var_type<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    typevar: TypeVarInstance<'db>,
    visitor: &V,
) {
    if let Some(bound_or_constraints) = if visitor.should_visit_lazy_type_attributes() {
        typevar.bound_or_constraints(db)
    } else {
        match typevar._bound_or_constraints(db) {
            _ if visitor.should_visit_lazy_type_attributes() => typevar.bound_or_constraints(db),
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints)) => {
                Some(bound_or_constraints)
            }
            _ => None,
        }
    } {
        walk_type_var_bounds(db, bound_or_constraints, visitor);
    }
    if let Some(default_type) = if visitor.should_visit_lazy_type_attributes() {
        typevar.default_type(db)
    } else {
        match typevar._default(db) {
            Some(TypeVarDefaultEvaluation::Eager(default_type)) => Some(default_type),
            _ => None,
        }
    } {
        visitor.visit_type(db, default_type);
    }
}

#[salsa::tracked]
impl<'db> TypeVarInstance<'db> {
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
            None,
        )
    }

    fn with_name_suffix(self, db: &'db dyn Db, suffix: &str) -> Self {
        Self::new(
            db,
            self.identity(db).with_name_suffix(db, suffix),
            self._bound_or_constraints(db),
            self.explicit_variance(db),
            self._default(db),
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

    pub(crate) fn upper_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        if let Some(TypeVarBoundOrConstraints::UpperBound(ty)) = self.bound_or_constraints(db) {
            Some(ty)
        } else {
            None
        }
    }

    pub(crate) fn constraints(self, db: &'db dyn Db) -> Option<&'db [Type<'db>]> {
        if let Some(TypeVarBoundOrConstraints::Constraints(tuple)) = self.bound_or_constraints(db) {
            Some(tuple.elements(db))
        } else {
            None
        }
    }

    pub(crate) fn bound_or_constraints(
        self,
        db: &'db dyn Db,
    ) -> Option<TypeVarBoundOrConstraints<'db>> {
        self._bound_or_constraints(db).and_then(|w| match w {
            TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints) => {
                Some(bound_or_constraints)
            }
            TypeVarBoundOrConstraintsEvaluation::LazyUpperBound => self
                .lazy_bound(db)
                .map(TypeVarBoundOrConstraints::UpperBound),
            TypeVarBoundOrConstraintsEvaluation::LazyConstraints => self
                .lazy_constraints(db)
                .map(TypeVarBoundOrConstraints::Constraints),
        })
    }

    /// Returns the bounds or constraints of this typevar. If the typevar is unbounded, returns
    /// `object` as its upper bound.
    pub(crate) fn require_bound_or_constraints(
        self,
        db: &'db dyn Db,
    ) -> TypeVarBoundOrConstraints<'db> {
        self.bound_or_constraints(db)
            .unwrap_or_else(|| TypeVarBoundOrConstraints::UpperBound(Type::object()))
    }

    pub(crate) fn default_type(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let visitor = TypeVarDefaultVisitor::new(None);
        self.default_type_impl(db, &visitor)
    }

    fn default_type_impl(
        self,
        db: &'db dyn Db,
        visitor: &TypeVarDefaultVisitor<'db>,
    ) -> Option<Type<'db>> {
        visitor.visit(self, || {
            self._default(db).and_then(|default| match default {
                TypeVarDefaultEvaluation::Eager(ty) => Some(ty),
                TypeVarDefaultEvaluation::Lazy => self.lazy_default_impl(db, visitor),
            })
        })
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
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
                        self.lazy_bound(db).map(|bound| {
                            TypeVarBoundOrConstraints::UpperBound(bound)
                                .materialize_impl(db, materialization_kind, visitor)
                                .into()
                        })
                    }
                    TypeVarBoundOrConstraintsEvaluation::LazyConstraints => {
                        self.lazy_constraints(db).map(|constraints| {
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
                    .lazy_default(db)
                    .map(|ty| ty.materialize(db, materialization_kind, visitor).into()),
            }),
        )
    }

    fn to_instance(self, db: &'db dyn Db) -> Option<Self> {
        let bound_or_constraints = match self.bound_or_constraints(db)? {
            TypeVarBoundOrConstraints::UpperBound(upper_bound) => {
                TypeVarBoundOrConstraints::UpperBound(upper_bound.to_instance(db)?)
            }
            TypeVarBoundOrConstraints::Constraints(constraints) => {
                TypeVarBoundOrConstraints::Constraints(constraints.to_instance(db)?)
            }
        };
        let identity = TypeVarIdentity::new(
            db,
            Name::new(format!("{}'instance", self.name(db))),
            None, // definition
            self.kind(db),
        );
        Some(Self::new(
            db,
            identity,
            Some(bound_or_constraints.into()),
            self.explicit_variance(db),
            None, // _default
        ))
    }

    fn type_is_self_referential(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
        visitor: &TypeVarDefaultVisitor<'db>,
    ) -> bool {
        #[derive(Copy, Clone)]
        struct State<'db, 'a> {
            db: &'db dyn Db,
            visitor: &'a TypeVarDefaultVisitor<'db>,
            seen_typevars: &'a RefCell<FxHashSet<TypeVarInstance<'db>>>,
            seen_type_aliases: &'a RefCell<FxHashSet<TypeAliasType<'db>>>,
        }

        fn typevar_default_is_self_referential<'db>(
            state: State<'db, '_>,
            typevar: TypeVarInstance<'db>,
            self_identity: TypeVarIdentity<'db>,
        ) -> bool {
            if typevar.identity(state.db) == self_identity {
                return true;
            }

            if !state.seen_typevars.borrow_mut().insert(typevar) {
                return false;
            }

            typevar
                .default_type_impl(state.db, state.visitor)
                .is_some_and(|default_ty| {
                    type_is_self_referential_impl(state, default_ty, self_identity)
                })
        }

        fn type_alias_is_self_referential<'db>(
            state: State<'db, '_>,
            type_alias: TypeAliasType<'db>,
            self_identity: TypeVarIdentity<'db>,
        ) -> bool {
            if !state.seen_type_aliases.borrow_mut().insert(type_alias) {
                return false;
            }

            type_is_self_referential_impl(state, type_alias.raw_value_type(state.db), self_identity)
        }

        fn type_is_self_referential_impl<'db>(
            state: State<'db, '_>,
            ty: Type<'db>,
            self_identity: TypeVarIdentity<'db>,
        ) -> bool {
            any_over_type(state.db, ty, false, |inner_ty| match inner_ty {
                Type::TypeVar(bound_typevar) => typevar_default_is_self_referential(
                    state,
                    bound_typevar.typevar(state.db),
                    self_identity,
                ),
                Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => {
                    typevar_default_is_self_referential(state, typevar, self_identity)
                }
                Type::TypeAlias(alias) => {
                    type_alias_is_self_referential(state, alias, self_identity)
                }
                Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) => {
                    type_alias_is_self_referential(state, alias, self_identity)
                }
                _ => false,
            })
        }

        let seen_typevars = RefCell::new(FxHashSet::default());
        let seen_type_aliases = RefCell::new(FxHashSet::default());

        let state = State {
            db,
            visitor,
            seen_typevars: &seen_typevars,
            seen_type_aliases: &seen_type_aliases,
        };

        type_is_self_referential_impl(state, ty, self.identity(db))
    }

    /// Returns the "unchecked" upper bound of a type variable instance.
    /// `lazy_bound` checks if the upper bound type is generic (generic upper bound is not allowed).
    #[salsa::tracked(
        cycle_fn=lazy_bound_cycle_recover,
        cycle_initial=|_, _, _| None,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn lazy_bound_unchecked(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let definition = self.definition(db)?;
        let module = parsed_module(db, definition.file(db)).load(db);
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

    fn lazy_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let bound = self.lazy_bound_unchecked(db)?;

        if bound.has_typevar_or_typevar_instance(db) {
            return None;
        }

        Some(bound)
    }

    /// Returns the "unchecked" constraints of a type variable instance.
    /// `lazy_constraints` checks if any of the constraint types are generic (generic constraints are not allowed).
    #[salsa::tracked(
        cycle_fn=lazy_constraints_cycle_recover,
        cycle_initial=|_, _, _| None,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn lazy_constraints_unchecked(self, db: &'db dyn Db) -> Option<TypeVarConstraints<'db>> {
        let definition = self.definition(db)?;
        let module = parsed_module(db, definition.file(db)).load(db);
        let constraints = match definition.kind(db) {
            // PEP 695 typevar
            DefinitionKind::TypeVar(typevar) => {
                let typevar_node = typevar.node(&module);
                let bound =
                    definition_expression_type(db, definition, typevar_node.bound.as_ref()?);
                let constraints = if let Some(tuple) = bound.tuple_instance_spec(db)
                    && let Tuple::Fixed(tuple) = tuple.into_owned()
                {
                    tuple.owned_elements()
                } else {
                    vec![Type::unknown()].into_boxed_slice()
                };
                TypeVarConstraints::new(db, constraints)
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

    fn lazy_constraints(self, db: &'db dyn Db) -> Option<TypeVarConstraints<'db>> {
        let constraints = self.lazy_constraints_unchecked(db)?;

        if constraints
            .elements(db)
            .iter()
            .any(|ty| ty.has_typevar_or_typevar_instance(db))
        {
            return None;
        }

        Some(constraints)
    }

    /// Returns the "unchecked" default type of a type variable instance.
    /// `lazy_default` checks if the default type is not self-referential.
    #[salsa::tracked(cycle_initial=|_, id, _| Some(Type::divergent(id)), cycle_fn=lazy_default_cycle_recover, heap_size=ruff_memory_usage::heap_size)]
    fn lazy_default_unchecked(self, db: &'db dyn Db) -> Option<Type<'db>> {
        fn convert_type_to_paramspec_value<'db>(db: &'db dyn Db, ty: Type<'db>) -> Type<'db> {
            let parameters = match ty {
                Type::NominalInstance(nominal_instance)
                    if nominal_instance.has_known_class(db, KnownClass::EllipsisType) =>
                {
                    Parameters::gradual_form()
                }
                Type::NominalInstance(nominal_instance) => nominal_instance
                    .own_tuple_spec(db)
                    .map_or_else(Parameters::unknown, |tuple_spec| {
                        Parameters::new(
                            db,
                            tuple_spec
                                .iter_all_elements()
                                .map(|ty| Parameter::positional_only(None).with_annotated_type(ty)),
                        )
                    }),
                Type::Dynamic(dynamic) => match dynamic {
                    DynamicType::Todo(_)
                    | DynamicType::TodoUnpack
                    | DynamicType::TodoStarredExpression
                    | DynamicType::TodoTypeVarTuple => Parameters::todo(),
                    DynamicType::Any
                    | DynamicType::Unknown
                    | DynamicType::UnknownGeneric(_)
                    | DynamicType::UnspecializedTypeVar
                    | DynamicType::InvalidConcatenateUnknown => Parameters::unknown(),
                },
                Type::Divergent(_) => Parameters::unknown(),
                Type::TypeVar(typevar) if typevar.is_paramspec(db) => {
                    return ty;
                }
                Type::KnownInstance(KnownInstanceType::TypeVar(typevar))
                    if typevar.is_paramspec(db) =>
                {
                    return ty;
                }
                _ => Parameters::unknown(),
            };
            Type::paramspec_value_callable(db, parameters)
        }

        let definition = self.definition(db)?;
        let module = parsed_module(db, definition.file(db)).load(db);
        let ty = match definition.kind(db) {
            // PEP 695 typevar
            DefinitionKind::TypeVar(typevar) => {
                let typevar_node = typevar.node(&module);
                definition_expression_type(db, definition, typevar_node.default.as_ref()?)
            }
            // legacy typevar / ParamSpec
            DefinitionKind::Assignment(assignment) => {
                let call_expr = assignment.value(&module).as_call_expr()?;
                let func_ty = definition_expression_type(db, definition, &call_expr.func);
                let known_class = func_ty.as_class_literal().and_then(|cls| cls.known(db));
                let expr = &call_expr.arguments.find_keyword("default")?.value;
                let default_type = definition_expression_type(db, definition, expr);
                if matches!(
                    known_class,
                    Some(KnownClass::ParamSpec | KnownClass::ExtensionsParamSpec)
                ) {
                    convert_type_to_paramspec_value(db, default_type)
                } else {
                    default_type
                }
            }
            // PEP 695 ParamSpec
            DefinitionKind::ParamSpec(paramspec) => {
                let paramspec_node = paramspec.node(&module);
                let default_ty =
                    definition_expression_type(db, definition, paramspec_node.default.as_ref()?);
                convert_type_to_paramspec_value(db, default_ty)
            }
            _ => return None,
        };

        Some(ty)
    }

    fn lazy_default(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let visitor = TypeVarDefaultVisitor::new(None);
        self.lazy_default_impl(db, &visitor)
    }

    fn lazy_default_impl(
        self,
        db: &'db dyn Db,
        visitor: &TypeVarDefaultVisitor<'db>,
    ) -> Option<Type<'db>> {
        let default = self.lazy_default_unchecked(db)?;

        // Unlike bounds/constraints, default types are allowed to be generic
        // (https://typing.python.org/en/latest/spec/generics.html#defaults-for-type-parameters).
        // Here we simply check for non-self-referential.
        // TODO: We should also check for non-forward references.
        if self.type_is_self_referential(db, default, visitor) {
            return None;
        }

        Some(default)
    }

    pub fn bind_pep695(self, db: &'db dyn Db) -> Option<BoundTypeVarInstance<'db>> {
        if !matches!(
            self.identity(db).kind(db),
            TypeVarKind::Pep695 | TypeVarKind::Pep695ParamSpec
        ) {
            return None;
        }
        let typevar_definition = self.definition(db)?;
        let index = semantic_index(db, typevar_definition.file(db));
        let (_, child) = index
            .child_scopes(typevar_definition.file_scope(db))
            .next()?;
        GenericContext::of_node(db, child.node(), index)?.binds_typevar(db, self)
    }
}

/// A nonce that gives a bound typevar occurrence a fresh identity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Update)]
pub struct TypeVarNonce(NonZeroU32);

// This type does not have any heap storage.
impl get_size2::GetSize for TypeVarNonce {}

impl TypeVarNonce {
    const FIRST: Self = Self(NonZeroU32::MIN);

    fn increment(self) -> Self {
        Self(
            self.0
                .checked_add(1)
                .expect("exhausted bound typevar freshness nonces"),
        )
    }
}

/// A clone-safe generator of fresh bound-typevar occurrence nonces.
#[derive(Clone, Debug)]
pub(crate) struct TypeVarNonceGenerator {
    next: Rc<Cell<TypeVarNonce>>,
}

impl Default for TypeVarNonceGenerator {
    fn default() -> Self {
        Self {
            next: Rc::new(Cell::new(TypeVarNonce::FIRST)),
        }
    }
}

impl TypeVarNonceGenerator {
    pub(crate) fn next(&self) -> TypeVarNonce {
        let nonce = self.next.get();
        self.next.set(nonce.increment());
        nonce
    }
}

/// A type variable that has been bound to a generic context, and which can be specialized to a
/// concrete type.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct BoundTypeVarInstance<'db> {
    pub typevar: TypeVarInstance<'db>,
    pub(super) binding_context: BindingContext<'db>,
    /// If [`Some`], this indicates that this type variable is the `args` or `kwargs` component
    /// of a `ParamSpec` i.e., `P.args` or `P.kwargs`.
    pub(super) paramspec_attr: Option<ParamSpecAttrKind>,
    /// If [`Some`], this bound typevar is a fresh occurrence of the source-level typevar.
    pub(super) freshness: Option<TypeVarNonce>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundTypeVarInstance<'_> {}

impl<'db> BoundTypeVarInstance<'db> {
    pub(crate) fn with_name_suffix(self, db: &'db dyn Db, suffix: &str) -> Self {
        Self::new(
            db,
            self.typevar(db).with_name_suffix(db, suffix),
            self.binding_context(db),
            self.paramspec_attr(db),
            self.freshness(db),
        )
    }

    /// Get the identity of this bound typevar occurrence.
    ///
    /// This is used for comparing whether two bound typevars represent the same occurrence,
    /// regardless of e.g. differences in their bounds or constraints due to materialization.
    pub(crate) fn identity(self, db: &'db dyn Db) -> BoundTypeVarIdentity<'db> {
        BoundTypeVarIdentity {
            identity: self.typevar(db).identity(db),
            binding_context: self.binding_context(db),
            paramspec_attr: self.paramspec_attr(db),
            freshness: self.freshness(db),
        }
    }

    /// Returns a new bound typevar instance with a fresh occurrence identity.
    #[expect(dead_code)]
    pub(crate) fn freshen(self, db: &'db dyn Db, nonce: TypeVarNonce) -> Self {
        Self::new(
            db,
            self.typevar(db),
            self.binding_context(db),
            self.paramspec_attr(db),
            Some(nonce),
        )
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db Name {
        self.typevar(db).name(db)
    }

    pub(crate) fn kind(self, db: &'db dyn Db) -> TypeVarKind {
        self.typevar(db).kind(db)
    }

    pub(crate) fn is_paramspec(self, db: &'db dyn Db) -> bool {
        self.kind(db).is_paramspec()
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

        let upper_bound = TypeVarBoundOrConstraints::UpperBound(match kind {
            ParamSpecAttrKind::Args => Type::homogeneous_tuple(db, Type::object()),
            ParamSpecAttrKind::Kwargs => KnownClass::Dict
                .to_specialized_instance(db, &[KnownClass::Str.to_instance(db), Type::any()])
                .top_materialization(db),
        });

        let typevar = TypeVarInstance::new(
            db,
            self.typevar(db).identity(db),
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(upper_bound)),
            None, // ParamSpecs cannot have explicit variance
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

        Self::new(
            db,
            TypeVarInstance::new(
                db,
                self.typevar(db).identity(db),
                None, // Remove the upper bound set by `with_paramspec_attr`
                None, // ParamSpecs cannot have explicit variance
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
    pub(crate) fn synthetic(db: &'db dyn Db, name: Name, variance: TypeVarVariance) -> Self {
        let identity = TypeVarIdentity::new(
            db,
            name,
            None, // definition
            TypeVarKind::Pep695,
        );
        let typevar = TypeVarInstance::new(
            db,
            identity,
            None, // _bound_or_constraints
            Some(variance),
            None, // _default
        );
        Self::new(db, typevar, BindingContext::Synthetic, None, None)
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
        Self::new(db, typevar, binding_context, None, None)
    }

    /// Returns an identical type variable with its `TypeVarBoundOrConstraints` mapped by the
    /// provided closure.
    pub(crate) fn map_bound_or_constraints(
        self,
        db: &'db dyn Db,
        f: impl FnOnce(Option<TypeVarBoundOrConstraints<'db>>) -> Option<TypeVarBoundOrConstraints<'db>>,
    ) -> Self {
        let bound_or_constraints = f(self.typevar(db).bound_or_constraints(db));
        let typevar = TypeVarInstance::new(
            db,
            self.typevar(db).identity(db),
            bound_or_constraints.map(TypeVarBoundOrConstraintsEvaluation::Eager),
            self.typevar(db).explicit_variance(db),
            self.typevar(db)._default(db),
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
                BindingContext::Definition(definition) => binding_type(db, definition)
                    .with_polarity(polarity)
                    .variance_of(db, self),
                BindingContext::Synthetic => TypeVarVariance::Invariant,
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
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Type<'db> {
        let mapped_specialization_type =
            |specialization: &ApplySpecialization<'a, 'db>| -> Option<Type<'db>> {
                let typevar = if self.is_paramspec(db) {
                    self.without_paramspec_attr(db)
                } else {
                    self
                };
                specialization.get(db, typevar).map(|ty| {
                    if let Some(attr) = self.paramspec_attr(db)
                        && let Type::TypeVar(typevar) = ty
                        && typevar.is_paramspec(db)
                    {
                        return Type::TypeVar(typevar.with_paramspec_attr(db, attr));
                    }
                    ty
                })
            };

        match type_mapping {
            TypeMapping::ApplySpecialization(specialization) => {
                mapped_specialization_type(specialization).unwrap_or(Type::TypeVar(self))
            }
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            } => mapped_specialization_type(specialization)
                .map(|mapped| {
                    // Only materialize if the specialization actually substituted this
                    // typevar with a different type. A typevar that maps back to itself
                    // hasn't been substituted and should not be materialized.
                    if mapped == Type::TypeVar(self) {
                        mapped
                    } else {
                        // Materialization uses a different mapping mode. Reuse of the outer
                        // visitor can incorrectly hit a cache entry from specialization.
                        let materialization_visitor = ApplyTypeMappingVisitor::default();
                        mapped.materialize(db, *materialization_kind, &materialization_visitor)
                    }
                })
                .unwrap_or(Type::TypeVar(self)),
            TypeMapping::BindSelf(binding) => {
                if binding.should_bind(db, self) {
                    binding.self_type()
                } else {
                    Type::TypeVar(self)
                }
            }
            TypeMapping::ReplaceSelf { new_upper_bound } => {
                if self.typevar(db).is_self(db) {
                    Type::TypeVar(BoundTypeVarInstance::synthetic_self(
                        db,
                        *new_upper_bound,
                        self.binding_context(db),
                    ))
                } else {
                    Type::TypeVar(self)
                }
            }
            TypeMapping::Promote(..)
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::EagerExpansion
            | TypeMapping::RescopeReturnCallables(_) => Type::TypeVar(self),
            TypeMapping::Materialize(materialization_kind) => {
                Type::TypeVar(self.materialize_impl(db, *materialization_kind, visitor))
            }
        }
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
        visitor: &ApplyTypeMappingVisitor<'db>,
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

    pub(super) fn to_instance(self, db: &'db dyn Db) -> Option<Self> {
        Some(Self::new(
            db,
            self.typevar(db).to_instance(db)?,
            self.binding_context(db),
            self.paramspec_attr(db),
            self.freshness(db),
        ))
    }
}

/// Whether this typevar was created via the legacy `TypeVar` constructor, using PEP 695 syntax,
/// or an implicit typevar like `Self` was used.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub enum TypeVarKind {
    /// `T = TypeVar("T")`
    Legacy,
    /// `def foo[T](x: T) -> T: ...`
    Pep695,
    /// `typing.Self`
    TypingSelf,
    /// `P = ParamSpec("P")`
    ParamSpec,
    /// `def foo[**P]() -> None: ...`
    Pep695ParamSpec,
    /// `Alias: typing.TypeAlias = T`
    Pep613Alias,
}

impl TypeVarKind {
    pub(super) const fn is_paramspec(self) -> bool {
        matches!(self, Self::ParamSpec | Self::Pep695ParamSpec)
    }
}

/// The identity of a type variable.
///
/// This represents the core identity of a typevar, independent of its bounds or constraints. Two
/// typevars have the same identity if they represent the same logical typevar, even if their
/// bounds have been materialized differently.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct TypeVarIdentity<'db> {
    /// The name of this TypeVar (e.g. `T`)
    #[returns(ref)]
    pub(crate) name: Name,

    /// The type var's definition (None if synthesized)
    pub(crate) definition: Option<Definition<'db>>,

    /// The kind of typevar (PEP 695, Legacy, or TypingSelf)
    pub(crate) kind: TypeVarKind,
}

impl get_size2::GetSize for TypeVarIdentity<'_> {}

impl<'db> TypeVarIdentity<'db> {
    fn with_name_suffix(self, db: &'db dyn Db, suffix: &str) -> Self {
        let name = format!("{}'{}", self.name(db), suffix);
        Self::new(db, Name::from(name), self.definition(db), self.kind(db))
    }
}

#[expect(clippy::ref_option)]
fn lazy_bound_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &Option<Type<'db>>,
    current: Option<Type<'db>>,
    _typevar: TypeVarInstance<'db>,
) -> Option<Type<'db>> {
    // Normalize the bounds/constraints to ensure cycle convergence.
    match (previous, current) {
        (Some(prev), Some(current)) => Some(current.cycle_normalized(db, *prev, cycle)),
        (None, Some(current)) => Some(current.recursive_type_normalized(db, cycle)),
        (_, None) => None,
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
#[expect(clippy::ref_option)]
fn lazy_constraints_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous: &Option<TypeVarConstraints<'db>>,
    current: Option<TypeVarConstraints<'db>>,
    _typevar: TypeVarInstance<'db>,
) -> Option<TypeVarConstraints<'db>> {
    // Normalize the bounds/constraints to ensure cycle convergence.
    match (previous, current) {
        (Some(prev), Some(constraints)) => Some(constraints.cycle_normalized(db, *prev, cycle)),
        (None, Some(current)) => Some(current.recursive_type_normalized(db, cycle)),
        (_, None) => None,
    }
}

#[expect(clippy::ref_option)]
fn lazy_default_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_default: &Option<Type<'db>>,
    default: Option<Type<'db>>,
    _typevar: TypeVarInstance<'db>,
) -> Option<Type<'db>> {
    // Normalize the default to ensure cycle convergence.
    match (previous_default, default) {
        (Some(prev), Some(default)) => Some(default.cycle_normalized(db, *prev, cycle)),
        (None, Some(default)) => Some(default.recursive_type_normalized(db, cycle)),
        (_, None) => None,
    }
}

/// Where a type variable is bound and usable.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, salsa::Update, get_size2::GetSize)]
pub enum BindingContext<'db> {
    /// The definition of the generic class, function, or type alias that binds this typevar.
    Definition(Definition<'db>),
    /// The typevar is synthesized internally, and is not associated with a particular definition
    /// in the source, but is still bound and eligible for specialization inference.
    Synthetic,
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
            BindingContext::Synthetic => None,
        }
    }

    pub(super) fn name(self, db: &'db dyn Db) -> Option<String> {
        self.definition().and_then(|definition| definition.name(db))
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, get_size2::GetSize)]
pub enum ParamSpecAttrKind {
    Args,
    Kwargs,
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
/// plus an optional freshness nonce for fresh callable occurrences, independent of the typevar's
/// bounds or constraints. Two bound typevars have the same identity if they represent the same
/// occurrence, even if their bounds have been materialized differently.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, get_size2::GetSize, salsa::Update)]
pub struct BoundTypeVarIdentity<'db> {
    pub(crate) identity: TypeVarIdentity<'db>,
    pub(crate) binding_context: BindingContext<'db>,
    /// If [`Some`], this indicates that this type variable is the `args` or `kwargs` component
    /// of a `ParamSpec` i.e., `P.args` or `P.kwargs`.
    pub(super) paramspec_attr: Option<ParamSpecAttrKind>,
    /// If [`Some`], this bound typevar is a fresh occurrence of the source-level typevar.
    pub(super) freshness: Option<TypeVarNonce>,
}

#[salsa::tracked(
    cycle_initial=|_, _, _| None,
    cycle_fn=bound_typevar_default_type_cycle_recover,
    heap_size=ruff_memory_usage::heap_size
)]
fn bound_typevar_default_type<'db>(
    db: &'db dyn Db,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<Type<'db>> {
    let binding_context = bound_typevar.binding_context(db);
    bound_typevar.typevar(db).default_type(db).map(|ty| {
        ty.apply_type_mapping(
            db,
            &TypeMapping::BindLegacyTypevars(binding_context),
            TypeContext::default(),
        )
    })
}

#[expect(clippy::ref_option)]
fn bound_typevar_default_type_cycle_recover<'db>(
    _db: &'db dyn Db,
    _cycle: &salsa::Cycle,
    _previous_default: &Option<Type<'db>>,
    _default: Option<Type<'db>>,
    _bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<Type<'db>> {
    None
}

/// Whether a typevar default is eagerly specified or lazily evaluated.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
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

/// Whether a typevar bound/constraints is eagerly specified or lazily evaluated.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
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

/// Type variable constraints (e.g. `T: (int, str)`).
/// This is structurally identical to [`UnionType`], except that it does not perform simplification and preserves the element types.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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
    pub(super) fn as_type(self, db: &'db dyn Db) -> Type<'db> {
        UnionType::from_elements(db, self.elements(db))
    }

    fn to_instance(self, db: &'db dyn Db) -> Option<TypeVarConstraints<'db>> {
        let mut instance_elements = Vec::new();
        for ty in self.elements(db) {
            instance_elements.push(ty.to_instance(db)?);
        }
        Some(TypeVarConstraints::new(
            db,
            instance_elements.into_boxed_slice(),
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
        mut transform_fn: impl FnMut(&Type<'db>) -> PlaceAndQualifiers<'db>,
    ) -> PlaceAndQualifiers<'db> {
        let mut builder = UnionBuilder::new(db);
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
                })
            },
            qualifiers,
        }
    }

    fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let materialized = self
            .elements(db)
            .iter()
            .map(|ty| ty.materialize(db, materialization_kind, visitor))
            .collect::<Box<_>>();
        TypeVarConstraints::new(db, materialized)
    }

    /// Normalize for cycle recovery by combining with the previous value and
    /// removing divergent types introduced by the cycle.
    ///
    /// See [`Type::cycle_normalized`] for more details on how this works.
    fn cycle_normalized(self, db: &'db dyn Db, previous: Self, cycle: &salsa::Cycle) -> Self {
        let current_elements = self.elements(db);
        let prev_elements = previous.elements(db);
        TypeVarConstraints::new(
            db,
            current_elements
                .iter()
                .zip(prev_elements.iter())
                .map(|(ty, prev_ty)| ty.cycle_normalized(db, *prev_ty, cycle))
                .collect::<Box<_>>(),
        )
    }

    /// Normalize recursive types for cycle recovery when there's no previous value.
    ///
    /// See [`Type::recursive_type_normalized`] for more details.
    fn recursive_type_normalized(self, db: &'db dyn Db, cycle: &salsa::Cycle) -> Self {
        self.map(db, |ty| ty.recursive_type_normalized(db, cycle))
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum TypeVarBoundOrConstraints<'db> {
    UpperBound(Type<'db>),
    Constraints(TypeVarConstraints<'db>),
}

pub(super) fn walk_type_var_bounds<'db, V: visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    bounds: TypeVarBoundOrConstraints<'db>,
    visitor: &V,
) {
    match bounds {
        TypeVarBoundOrConstraints::UpperBound(bound) => visitor.visit_type(db, bound),
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
        visitor: &ApplyTypeMappingVisitor<'db>,
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

    /// Represent the bound/constraints of this typevar as a single type, by unioning constraints.
    ///
    /// Careful with this method! It has both semantic and performance gotchas. Unioning
    /// constraints provides a conservative upper bound, but it loses precision. And for many use
    /// cases, it's more efficient to just map over the constraint types directly, rather than
    /// building a union out of them and mapping over that.
    pub(crate) fn as_type(self, db: &'db dyn Db) -> Type<'db> {
        match self {
            TypeVarBoundOrConstraints::UpperBound(bound) => bound,
            TypeVarBoundOrConstraints::Constraints(constraints) => constraints.as_type(db),
        }
    }
}

/// A [`CycleDetector`] that is used in `TypeVarInstance::default_type`.
pub(crate) type TypeVarDefaultVisitor<'db> =
    CycleDetector<VisitTypeVarDefault, TypeVarInstance<'db>, Option<Type<'db>>>;
pub(crate) struct VisitTypeVarDefault;
