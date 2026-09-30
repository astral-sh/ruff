use crate::ProgramEnvironment;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use itertools::{Either, Itertools};
use ruff_db::parsed::parsed_module;
use ruff_python_ast::name::Name;
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::{
    Db, FxOrderMap, TypeQualifiers,
    place::{
        DefinedPlace, Definedness, Place, PlaceAndQualifiers, Provenance, PublicTypePolicy,
        TypeOrigin,
    },
    types::{
        ApplySpecialization, ApplyTypeMappingVisitor, CycleDetector, DynamicType, GenericContext,
        InstanceProjection, IntersectionType, KnownClass, KnownInstanceType, MaterializationKind,
        Parameter, Parameters, Type, TypeAliasType, TypeContext, TypeMapping, TypeVarVariance,
        UnionBuilder, UnionType, any_over_type, any_over_type_including_alias_arguments,
        binding_type,
        cyclic::TypeIdentity,
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
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
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

    /// The default type for this TypeVar, if any. Don't use this field directly, use the
    /// `default_type` method instead (to evaluate any lazy default).
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
    if let Some(bound_or_constraints) = if visitor.should_visit_lazy_type_attributes() {
        typevar.bound_or_constraints(db, visitor.program_environment())
    } else {
        match typevar._bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints)) => {
                Some(bound_or_constraints)
            }
            Some(
                TypeVarBoundOrConstraintsEvaluation::LazyUpperBound
                | TypeVarBoundOrConstraintsEvaluation::LazyConstraints,
            ) => {
                visitor.notify_skipped_lazy_type_attributes();
                None
            }
            _ => None,
        }
    } {
        walk_type_var_bounds(db, bound_or_constraints, visitor);
    }
    if let Some(default_type) = if visitor.should_visit_lazy_type_attributes() {
        typevar.default_type(db, visitor.program_environment())
    } else {
        match typevar._default(db) {
            Some(TypeVarDefaultEvaluation::Eager(default_type)) => Some(default_type),
            Some(TypeVarDefaultEvaluation::Lazy) => {
                visitor.notify_skipped_lazy_type_attributes();
                None
            }
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
            TypeVarNonce::NONE,
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
        self._bound_or_constraints(db).and_then(|w| match w {
            TypeVarBoundOrConstraintsEvaluation::Eager(bound_or_constraints) => {
                Some(bound_or_constraints)
            }
            TypeVarBoundOrConstraintsEvaluation::LazyUpperBound => self
                .lazy_bound(db, env)
                .map(TypeVarBoundOrConstraints::UpperBound),
            TypeVarBoundOrConstraintsEvaluation::LazyConstraints => self
                .lazy_constraints(db, env)
                .map(TypeVarBoundOrConstraints::Constraints),
        })
    }

    pub(crate) fn default_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<Type<'db>> {
        let visitor = TypeVarDefaultVisitor::new(None);
        self.default_type_impl(db, env, &visitor)
    }

    fn default_type_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        visitor: &TypeVarDefaultVisitor<'db>,
    ) -> Option<Type<'db>> {
        visitor.visit(db, self, || {
            self._default(db).and_then(|default| match default {
                TypeVarDefaultEvaluation::Eager(ty) => Some(ty),
                TypeVarDefaultEvaluation::Lazy => self.lazy_default_impl(db, env, visitor),
            })
        })
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
        type SeenTypes<'db> = SmallVec<[TypeIdentity<'db>; 1]>;

        #[derive(Copy, Clone)]
        struct State<'db, 'a> {
            db: &'db dyn Db,
            env: &'a ProgramEnvironment<'db>,
            visitor: &'a TypeVarDefaultVisitor<'db>,
            seen_typevars: &'a RefCell<FxHashSet<TypeVarInstance<'db>>>,
            seen_types: &'a RefCell<SeenTypes<'db>>,
        }

        fn typevar_default_is_self_referential<'db>(
            state: State<'db, '_>,
            typevar: TypeVarInstance<'db>,
            self_identity: TypeVarIdentity<'db>,
        ) -> bool {
            let db = state.db;

            if typevar.identity(db) == self_identity {
                return true;
            }

            if !state.seen_typevars.borrow_mut().insert(typevar) {
                return false;
            }

            typevar
                .default_type_impl(db, state.env, state.visitor)
                .is_some_and(|default_ty| {
                    type_is_self_referential_impl(state, default_ty, self_identity)
                })
        }

        fn type_alias_is_self_referential<'db>(
            state: State<'db, '_>,
            type_alias: TypeAliasType<'db>,
            self_identity: TypeVarIdentity<'db>,
        ) -> bool {
            let db = state.db;
            let specialization = type_alias.specialization(db);
            // A nested specialization can contain self even when its alias body was already visited.
            if let Some(specialization) = specialization {
                if specialization
                    .types(db)
                    .iter()
                    .any(|ty| type_is_self_referential_impl(state, *ty, self_identity))
                {
                    return true;
                }
            } else if let Some(generic_context) = type_alias.generic_context(db)
                && generic_context.variables(db).any(|typevar| {
                    typevar_default_is_self_referential(state, typevar.typevar(db), self_identity)
                })
            {
                return true;
            }

            {
                let mut seen_types = state.seen_types.borrow_mut();
                // The shared recursive identity also stops specializations that keep growing.
                let identity = Type::TypeAlias(type_alias).to_type_identity(db);
                if seen_types.contains(&identity) {
                    return false;
                }
                seen_types.push(identity);
            }

            let value_type = if specialization.is_some() {
                type_alias.value_type(db)
            } else {
                type_alias.raw_value_type(db)
            };
            type_is_self_referential_impl(state, value_type, self_identity)
        }

        fn type_is_self_referential_impl<'db>(
            state: State<'db, '_>,
            ty: Type<'db>,
            self_identity: TypeVarIdentity<'db>,
        ) -> bool {
            let db = state.db;
            any_over_type(db, state.env, ty, false, |inner_ty| match inner_ty {
                Type::TypeVar(bound_typevar) => typevar_default_is_self_referential(
                    state,
                    bound_typevar.typevar(db),
                    self_identity,
                ),
                Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) => {
                    typevar_default_is_self_referential(state, typevar, self_identity)
                }
                Type::TypeAlias(alias) => {
                    type_alias_is_self_referential(state, alias, self_identity)
                }
                Type::Recursive(recursive) => {
                    if recursive.arguments(db).is_some_and(|arguments| {
                        arguments
                            .types(db)
                            .iter()
                            .any(|ty| type_is_self_referential_impl(state, *ty, self_identity))
                    }) {
                        return true;
                    }
                    {
                        let mut seen_types = state.seen_types.borrow_mut();
                        let identity = Type::Recursive(recursive).to_type_identity(db);
                        if seen_types.contains(&identity) {
                            return false;
                        }
                        seen_types.push(identity);
                    }
                    type_is_self_referential_impl(
                        state,
                        recursive.unfold(db, state.env).into_type(),
                        self_identity,
                    )
                }
                Type::KnownInstance(KnownInstanceType::TypeAliasType(alias)) => {
                    type_alias_is_self_referential(state, alias, self_identity)
                }
                _ => false,
            })
        }

        let seen_typevars = RefCell::new(FxHashSet::default());
        let seen_types = RefCell::new(SeenTypes::new());

        let state = State {
            db,
            env,
            visitor,
            seen_typevars: &seen_typevars,
            seen_types: &seen_types,
        };

        type_is_self_referential_impl(state, ty, self.identity(db))
    }

    /// Returns the "unchecked" upper bound of a type variable instance.
    /// `lazy_bound` checks if the upper bound type is generic (generic upper bound is not allowed).
    #[salsa::tracked(
        returns(copy),
        cycle_fn=lazy_bound_cycle_recover,
        cycle_initial=|_, _, _| None,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn lazy_bound_unchecked(self, db: &'db dyn Db) -> Option<Type<'db>> {
        let definition = self.definition(db)?;
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

    fn lazy_bound(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Option<Type<'db>> {
        let bound = self.lazy_bound_unchecked(db)?;

        if bound.has_typevar_or_typevar_instance(db, env) {
            return None;
        }

        Some(bound)
    }

    /// Returns the "unchecked" constraints of a type variable instance.
    /// `lazy_constraints` checks if any of the constraint types are generic (generic constraints are not allowed).
    #[salsa::tracked(
        returns(copy),
        cycle_fn=lazy_constraints_cycle_recover,
        cycle_initial=|_, _, _| None,
        heap_size=ruff_memory_usage::heap_size
    )]
    fn lazy_constraints_unchecked(self, db: &'db dyn Db) -> Option<TypeVarConstraints<'db>> {
        let definition = self.definition(db)?;
        let program_file = definition.program_file(db);
        let python_file = program_file.python_file(db);
        let env = ProgramEnvironment::from_file(program_file);
        let module = parsed_module(db, python_file).load(db);
        let constraints = match definition.kind(db) {
            // PEP 695 typevar
            DefinitionKind::TypeVar(typevar) => {
                let typevar_node = typevar.node(&module);
                let bound =
                    definition_expression_type(db, definition, typevar_node.bound.as_ref()?);
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
    #[salsa::tracked(returns(copy), cycle_initial=|_, id, _| Some(Type::divergent(id)), cycle_fn=lazy_default_cycle_recover, heap_size=ruff_memory_usage::heap_size)]
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
                        match tuple_spec.as_ref() {
                            Tuple::Fixed(tuple) => {
                                Parameters::standard(tuple.iter_all_elements().map(|ty| {
                                    Parameter::positional_only(None).with_annotated_type(ty)
                                }))
                            }
                            // A `ParamSpec` default cannot contain a variable-length tuple, so this
                            // branch only recovers from an invalid type expression.
                            Tuple::Variable(_) => Parameters::unknown(),
                        }
                    }),
                Type::Dynamic(dynamic) => match dynamic {
                    DynamicType::Todo(_) => Parameters::todo(),
                    DynamicType::Any
                    | DynamicType::Unknown
                    | DynamicType::UnknownGeneric(_)
                    | DynamicType::UnspecializedTypeVar
                    | DynamicType::UnknownLambdaParameter
                    | DynamicType::InvalidConcatenateUnknown
                    | DynamicType::AmbiguousOverload => Parameters::unknown(),
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
        let program_file = definition.program_file(db);
        let python_file = program_file.python_file(db);
        let module = parsed_module(db, python_file).load(db);
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
            // PEP 695 TypeVarTuple
            DefinitionKind::TypeVarTuple(typevartuple) => {
                let typevartuple_node = typevartuple.node(&module);
                definition_expression_type(db, definition, typevartuple_node.default.as_ref()?)
            }
            _ => return None,
        };

        Some(ty)
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
        let default = self.lazy_default_unchecked(db)?;

        // Unlike bounds/constraints, default types are allowed to be generic
        // (https://typing.python.org/en/latest/spec/generics.html#defaults-for-type-parameters).
        // Here we simply check for non-self-referential.
        // TODO: We should also check for non-forward references.
        if self.type_is_self_referential(db, env, default, visitor) {
            return None;
        }

        Some(default)
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
/// The generator only allocates a nonce for the second and later occurrence of a generic context.
/// The first occurrence can use its source-level identity directly because there is no previous
/// occurrence for it to collide with.
#[derive(Clone, Debug)]
pub(crate) struct TypeVarNonceGenerator<'db> {
    inner: Rc<RefCell<TypeVarNonceGeneratorInner<'db>>>,
}

impl Default for TypeVarNonceGenerator<'_> {
    fn default() -> Self {
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
    pub(crate) fn record_enclosing_binding_contexts(
        &self,
        binding_contexts: impl IntoIterator<Item = BindingContext<'db>>,
    ) {
        let mut inner = self.inner.borrow_mut();
        inner.enclosing.extend(binding_contexts);
    }

    pub(crate) fn should_freshen(
        &self,
        db: &'db dyn Db,
        generic_context: GenericContext<'db>,
    ) -> bool {
        let mut inner = self.inner.borrow_mut();
        let mut binding_contexts = generic_context
            .variables(db)
            .map(|typevar| typevar.binding_context(db));
        // A context inherited from an enclosing definition can be merged with another context.
        // Only the unmerged context represents a recursive occurrence that needs freshening.
        let matches_enclosing = binding_contexts.next().is_some_and(|binding_context| {
            inner.enclosing.contains(&binding_context)
                && binding_contexts.all(|other| other == binding_context)
        });
        matches_enclosing || !inner.seen.insert(generic_context)
    }

    pub(crate) fn next(&self) -> TypeVarNonce {
        let mut inner = self.inner.borrow_mut();
        let nonce = inner.next;
        inner.next = nonce.increment();
        nonce
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
                    // Freshening can also change the domain when it refers to another
                    // freshened typevar. Conservatively match all domains of the same binder
                    // so that a subsequent freshening cannot reuse an existing nonce.
                    identity.canonical_domain = None;
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
            identity.canonical_domain = None;
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
#[salsa::interned(
    debug,
    constructor = new_internal,
    heap_size = ruff_memory_usage::heap_size
)]
pub struct BoundTypeVarInstance<'db> {
    #[returns(copy)]
    pub typevar: TypeVarInstance<'db>,
    /// The declaration before specialization, materialization, or transposition of its domain.
    #[returns(copy)]
    original: TypeVarInstance<'db>,
    // This duplicates the source-level identity accessible through `typevar`, but keeps
    // `identity()` to a single interned-field read. Storing only the occurrence-specific fields
    // and reconstructing the full identity regresses hot-path project benchmarks.
    #[returns(copy)]
    identity_inner: BoundTypeVarIdentity<'db>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for BoundTypeVarInstance<'_> {}

impl<'db> BoundTypeVarInstance<'db> {
    fn canonical_domain_key(
        db: &'db dyn Db,
        typevar: TypeVarInstance<'db>,
        domain: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
    ) -> TypeVarInstance<'db> {
        TypeVarInstance::new(db, typevar.identity(db), domain, None, None)
    }

    pub(crate) fn new(
        db: &'db dyn Db,
        typevar: TypeVarInstance<'db>,
        binding_context: BindingContext<'db>,
        paramspec_attr: Option<ParamSpecAttrKind>,
        freshness: TypeVarNonce,
    ) -> Self {
        let identity = BoundTypeVarIdentity {
            identity: typevar.identity(db),
            binding_context,
            paramspec_attr,
            freshness,
            canonical_domain: typevar.is_self(db).then(|| {
                Self::canonical_domain_key(db, typevar, typevar._bound_or_constraints(db))
            }),
        };
        Self::new_internal(db, typevar, typevar, identity)
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
        let typevar = self.typevar(db).with_name_suffix(db, suffix);
        let mut identity = self.identity(db);
        identity.identity = typevar.identity(db);
        identity.canonical_domain = identity
            .canonical_domain
            .map(|domain| domain.with_name_suffix(db, suffix));
        Self::new_internal(
            db,
            typevar,
            self.original(db).with_name_suffix(db, suffix),
            identity,
        )
    }

    /// Get the identity of this bound typevar occurrence.
    ///
    /// This includes the source-level typevar, binding context, `ParamSpec` attribute, freshness
    /// nonce, and any specialized domain. Materialized views of the same occurrence retain the
    /// same identity.
    pub(crate) fn identity(self, db: &'db dyn Db) -> BoundTypeVarIdentity<'db> {
        self.identity_inner(db)
    }

    pub(crate) fn name(self, db: &'db dyn Db) -> &'db Name {
        self.typevar(db).name(db)
    }

    pub(crate) fn kind(self, db: &'db dyn Db) -> TypeVarKind {
        self.identity(db).kind(db)
    }

    pub(crate) fn domain(self, db: &'db dyn Db) -> TypeVarDomain {
        let identity = self.identity(db);
        let kind = identity.kind(db);
        if kind.is_paramspec() && identity.paramspec_attr.is_none() {
            TypeVarDomain::ParameterSignature
        } else if kind.is_typevartuple() {
            TypeVarDomain::TypeTuple
        } else {
            TypeVarDomain::Type
        }
    }

    pub(crate) fn is_paramspec(self, db: &'db dyn Db) -> bool {
        self.kind(db).is_paramspec()
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

        let mut identity = self.identity(db);
        identity.paramspec_attr = Some(kind);
        Self::new_internal(db, typevar, typevar, identity)
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
        let typevar = TypeVarInstance::new(
            db,
            typevar.identity(db),
            None, // Remove the upper bound set by `with_paramspec_attr`
            typevar.explicit_variance(db),
            None, // `P.args` and `P.kwargs` cannot have defaults even though `P` can
        );
        Self::new_internal(
            db,
            typevar,
            typevar,
            self.identity(db).without_paramspec_attr(db),
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

    /// Whether a specialization may affect a represented bound without forcing a lazy bound.
    pub(super) fn specialization_may_change_domain(self, db: &'db dyn Db) -> bool {
        self.identity(db).canonical_domain.is_some()
            || [self.typevar(db), self.original(db)]
                .into_iter()
                .any(|typevar| {
                    matches!(
                        typevar._bound_or_constraints(db),
                        Some(TypeVarBoundOrConstraintsEvaluation::Eager(_))
                    )
                })
    }

    fn apply_type_mapping_to_bound_or_constraints(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        self.map_bound_or_constraints(db, |original| {
            original.map(|original| original.apply_type_mapping_impl(db, type_mapping, visitor))
        })
    }

    /// Maps a binder's domain and updates its identity to represent the new specialization.
    ///
    /// The current bound may be a materialized or transposed view of the declared domain. Map
    /// that view independently, and derive the new identity from the canonical domain stored in
    /// the existing identity. The mapping must respect the scope of variables in the domain.
    fn map_domain(
        self,
        db: &'db dyn Db,
        view: Self,
        type_mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let mapped = view.apply_type_mapping_to_bound_or_constraints(db, type_mapping, visitor);
        self.update_domain_after_mapping(db, mapped, type_mapping, visitor)
    }

    fn update_domain_after_mapping(
        self,
        db: &'db dyn Db,
        mapped: Self,
        type_mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        if matches!(type_mapping, TypeMapping::Materialize(_)) {
            return mapped;
        }
        let mut identity = mapped.identity(db);
        let original = self.original(db);
        let canonical = self.identity(db).canonical_domain.unwrap_or(original);
        let previous = canonical.bound_or_constraints(db, visitor.env);
        // A specialization can materialize the types it substitutes. Only the visible view uses
        // that materialization; the identity tracks the specialization before materialization.
        let canonical_mapping = match type_mapping {
            TypeMapping::ApplySpecializationWithMaterialization { specialization, .. } => {
                TypeMapping::ApplySpecialization(*specialization)
            }
            _ => type_mapping.clone(),
        };
        let current =
            previous.map(|domain| domain.apply_type_mapping_impl(db, &canonical_mapping, visitor));
        if current != previous {
            identity.canonical_domain = if current == original.bound_or_constraints(db, visitor.env)
            {
                original.is_self(db).then(|| {
                    Self::canonical_domain_key(db, original, original._bound_or_constraints(db))
                })
            } else {
                Some(Self::canonical_domain_key(
                    db,
                    original,
                    current.map(TypeVarBoundOrConstraintsEvaluation::Eager),
                ))
            };
        }
        Self::new_internal(db, mapped.typevar(db), original, identity)
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

        Self::new_internal(db, typevar, self.original(db), self.identity(db))
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

        let apply_retained_domain = |specialization: &ApplySpecialization<'a, 'db>,
                                     mapped: Type<'db>| {
            if let Type::TypeVar(view) = mapped
                && view.is_same_typevar_as(db, self)
                && specialization.specializes_typevar_domains()
                && (self.specialization_may_change_domain(db)
                    || view.specialization_may_change_domain(db))
            {
                let mapping = TypeMapping::ApplySpecialization(*specialization);
                Type::TypeVar(self.map_domain(db, view, &mapping, visitor))
            } else {
                mapped
            }
        };

        match type_mapping {
            TypeMapping::ApplySpecialization(specialization) => {
                let mapped =
                    mapped_specialization_type(specialization).unwrap_or(Type::TypeVar(self));
                apply_retained_domain(specialization, mapped)
            }
            TypeMapping::ApplySpecializationWithMaterialization {
                specialization,
                materialization_kind,
            } => {
                let mapped = mapped_specialization_type(specialization)
                    .map(|mapped| {
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
                    })
                    .unwrap_or(Type::TypeVar(self));
                apply_retained_domain(specialization, mapped)
            }
            TypeMapping::BindSelf(binding) => {
                if binding.should_bind(db, visitor.env, self) {
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
            TypeMapping::FreshenBoundTypeVars {
                generic_context,
                delta,
            } => {
                if generic_context.contains(db, self.identity(db)) && !self.is_paramspec(db) {
                    let freshened = self.freshen_with_mapping(
                        db,
                        self.freshness(db).add(*delta),
                        type_mapping,
                        visitor,
                    );
                    Type::TypeVar(self.update_domain_after_mapping(
                        db,
                        freshened,
                        type_mapping,
                        visitor,
                    ))
                } else if self.specialization_may_change_domain(db) {
                    Type::TypeVar(self.map_domain(db, self, type_mapping, visitor))
                } else {
                    Type::TypeVar(self)
                }
            }
            TypeMapping::Promote(..)
            | TypeMapping::ReplaceParameterDefaults
            | TypeMapping::BindLegacyTypevars(_)
            | TypeMapping::EagerExpansion
            | TypeMapping::RescopeReturnCallables(_)
            | TypeMapping::ApplyRecursiveSubstitution(_) => Type::TypeVar(self),
            TypeMapping::Materialize(materialization_kind) => {
                if visitor.materialize_typevar_bounds_and_defaults {
                    Type::TypeVar(self.materialize_impl(db, *materialization_kind, visitor))
                } else {
                    Type::TypeVar(self)
                }
            }
        }
    }

    /// Returns the static upper bound used when materializing a gradual type argument.
    ///
    /// Constraints are unioned only when materializing an exposed member, where their union is a
    /// valid conservative upper bound. A bound may recursively refer to its own generic class,
    /// either directly or through other bounds. Such a bound has no finite static top
    /// materialization, so recover from its cycle without applying an upper bound.
    pub(super) fn top_materialized_upper_bound(self, db: &'db dyn Db) -> Option<Type<'db>> {
        #[salsa::tracked(
            returns(copy),
            cycle_result=|_, _, _| None,
            heap_size=ruff_memory_usage::heap_size
        )]
        fn top_materialized_upper_bound_inner<'db>(
            db: &'db dyn Db,
            bound_typevar: BoundTypeVarInstance<'db>,
        ) -> Option<Type<'db>> {
            let env =
                ProgramEnvironment::from_program(bound_typevar.binding_context(db).program(db));

            bound_typevar
                .typevar(db)
                .bound_or_constraints(db, &env)
                .map(|bound_or_constraints| {
                    bound_or_constraints
                        .as_type(db, &env)
                        .top_materialization(db, &env)
                })
        }

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
        Self::new_internal(
            db,
            self.typevar(db)
                .materialize_impl(db, materialization_kind, visitor),
            self.original(db),
            self.identity(db),
        )
    }

    fn freshen_with_mapping(
        self,
        db: &'db dyn Db,
        nonce: TypeVarNonce,
        type_mapping: &TypeMapping<'_, 'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        let typevar = self.typevar(db);
        let bound_or_constraints = typevar.bound_or_constraints(db, visitor.env);
        let default = self.default_type(db);

        if bound_or_constraints.is_none() && default.is_none() {
            let mut identity = self.identity(db);
            identity.freshness = nonce;
            return Self::new_internal(db, typevar, self.original(db), identity);
        }

        let typevar = TypeVarInstance::new(
            db,
            typevar.identity(db),
            bound_or_constraints.map(|bound_or_constraints| {
                bound_or_constraints
                    .apply_type_mapping_impl(db, type_mapping, visitor)
                    .into()
            }),
            typevar.explicit_variance(db),
            default.map(|ty| {
                ty.apply_type_mapping_impl(db, type_mapping, TypeContext::default(), visitor)
                    .into()
            }),
        );

        let mut identity = self.identity(db);
        identity.freshness = nonce;
        Self::new_internal(db, typevar, self.original(db), identity)
    }

    pub(super) fn to_instance(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<InstanceProjection<Self>> {
        Some(self.typevar(db).to_instance(db, env)?.map(|typevar| {
            let mut identity = self.identity(db);
            identity.identity = typevar.identity(db);
            Self::new_internal(db, typevar, self.original(db), identity)
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
    #[returns(copy)]
    pub(crate) definition: Option<Definition<'db>>,

    /// The kind of typevar (PEP 695, Legacy, or TypingSelf)
    #[returns(copy)]
    pub(crate) kind: TypeVarKind,
}

impl get_size2::GetSize for TypeVarIdentity<'_> {}

impl<'db> TypeVarIdentity<'db> {
    fn with_name_suffix(self, db: &'db dyn Db, suffix: &str) -> Self {
        let name = Name::concat(&[self.name(db).as_str(), "'", suffix]);
        Self::new(db, name, self.definition(db), self.kind(db))
    }
}

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
    // Normalize the default to ensure cycle convergence.
    let current = current?;
    let program_file = typevar
        .definition(db)
        .expect("a lazy TypeVar default must have a source definition")
        .program_file(db);
    let env = ProgramEnvironment::from_file(program_file);
    Some(match previous_default {
        Some(prev) => current.cycle_normalized(db, &env, *prev, cycle),
        None => current.recursive_type_normalized(db, &env, cycle),
    })
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
/// plus a freshness nonce for fresh callable occurrences. Its specialized canonical domain
/// distinguishes bindings with different specialized bounds or constraints. Two bound
/// typevars have the same identity if they represent the same occurrence, even if their bounds
/// have been materialized differently. Two fresh occurrences of the same source-level typevar
/// have different bound identities.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct BoundTypeVarIdentity<'db> {
    pub(crate) identity: TypeVarIdentity<'db>,
    pub(crate) binding_context: BindingContext<'db>,
    /// If [`Some`], this indicates that this type variable is the `args` or `kwargs` component
    /// of a `ParamSpec` i.e., `P.args` or `P.kwargs`.
    pub(super) paramspec_attr: Option<ParamSpecAttrKind>,
    /// The freshness nonce for this bound typevar occurrence; `0` is the source-level occurrence.
    freshness: TypeVarNonce,
    /// The specialized domain before identity-preserving transformations such as materialization
    /// and transposition. `Self` also records its original domain to distinguish synthetic binders.
    canonical_domain: Option<TypeVarInstance<'db>>,
}

impl<'db> BoundTypeVarIdentity<'db> {
    fn kind(self, db: &'db dyn Db) -> TypeVarKind {
        self.identity.kind(db)
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
        let mut typevars = typevars.into_iter().peekable();
        if typevars.peek().is_none() {
            return TypeVarSet::None;
        }

        let mut set = FxOrderMap::default();
        for typevar in typevars {
            set.entry(typevar.identity(db)).or_insert(typevar);
        }
        set.shrink_to_fit();
        Self::Some(TypeVarSetInner::new_internal(db, set))
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

#[salsa::tracked(
    returns(copy),
    cycle_initial=|_, id, _| Some(Type::divergent(id)),
    cycle_fn=bound_typevar_default_type_cycle_recover,
    heap_size=ruff_memory_usage::heap_size
)]
fn bound_typevar_default_type<'db>(
    db: &'db dyn Db,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<Type<'db>> {
    let typevar = bound_typevar.typevar(db);
    typevar._default(db)?;
    let definition = typevar
        .definition(db)
        .expect("a bound TypeVar with a default must have a source definition");
    let env = ProgramEnvironment::from_definition(definition);
    let default = typevar.default_type(db, &env)?;
    let binding_context = bound_typevar.binding_context(db);

    Some(default.apply_type_mapping(
        db,
        &env,
        &TypeMapping::BindLegacyTypevars(binding_context),
        TypeContext::default(),
    ))
}

#[expect(clippy::ref_option)]
fn bound_typevar_default_type_cycle_recover<'db>(
    db: &'db dyn Db,
    cycle: &salsa::Cycle,
    previous_default: &Option<Type<'db>>,
    default: Option<Type<'db>>,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> Option<Type<'db>> {
    let default = default?;
    let program_file = bound_typevar
        .typevar(db)
        .definition(db)
        .expect("a bound TypeVar with a default must have a source definition")
        .program_file(db);
    let env = ProgramEnvironment::from_file(program_file);
    Some(match previous_default {
        Some(previous) => default.cycle_normalized(db, &env, *previous, cycle),
        None => default.recursive_type_normalized(db, &env, cycle),
    })
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
    pub(super) fn as_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        UnionType::from_elements(db, env, self.elements(db))
    }

    /// Whether every constraint in `self` has an equivalent constraint in `other`.
    ///
    /// For example, `(int, str)` is a subset of `(int, str, bytes)`, but `(bool, str)` is not
    /// a subset of `(int, str)`: `bool` is a subtype of `int`, but is not equivalent to it.
    pub(super) fn is_subset_of(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Self,
    ) -> bool {
        self.elements(db).iter().all(|constraint| {
            other.elements(db).iter().any(|other_constraint| {
                // Union types preserve element order, so `int | str` and `str | int` can
                // compare unequal with `==` even though they are equivalent constraints.
                constraint.is_equivalent_to(db, env, *other_constraint)
            })
        })
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

    fn map_self<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarInstance<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
    ) -> BoundTypeVarInstance<'db> {
        typevar.map_domain(
            db,
            typevar,
            type_mapping,
            &ApplyTypeMappingVisitor::new(env),
        )
    }

    fn specialization_mapping<'db>(
        db: &'db dyn Db,
        context: GenericContext<'db>,
        ty: Type<'db>,
    ) -> TypeMapping<'db, 'db> {
        TypeMapping::ApplySpecialization(ApplySpecialization::specialization(
            context.specialize(db, vec![ty]),
        ))
    }

    fn specialize_typevar<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        typevar: BoundTypeVarInstance<'db>,
        context: GenericContext<'db>,
        ty: Type<'db>,
    ) -> BoundTypeVarInstance<'db> {
        typevar
            .apply_type_mapping_impl(
                db,
                &specialization_mapping(db, context, ty),
                &ApplyTypeMappingVisitor::new(env),
            )
            .as_typevar()
            .expect("the retained typevar should remain a typevar")
    }

    #[test]
    fn specialized_typevar_domains_distinguish_bindings() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let u = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let v = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("V"),
            TypeVarVariance::Invariant,
        );
        let u_context = GenericContext::from_typevar_instances(db, &env, [u]);
        let v_context = GenericContext::from_typevar_instances(db, &env, [v]);
        let int = KnownClass::Int.to_instance(db, &env);
        let str = KnownClass::Str.to_instance(db, &env);
        let bounds = [
            TypeVarBoundOrConstraints::UpperBound(KnownClass::List.to_specialized_instance(
                db,
                &env,
                &[Type::TypeVar(u)],
            )),
            TypeVarBoundOrConstraints::Constraints(TypeVarConstraints::new(
                db,
                [Type::TypeVar(u), str].as_slice(),
            )),
        ];
        for bound in bounds {
            let t = bound_typevar(
                db,
                &env,
                "T",
                TypeVarKind::Pep695TypeVar,
                Some(bound.into()),
                TypeVarNonce::NONE,
            );
            let no_op = specialize_typevar(db, &env, t, u_context, Type::TypeVar(u));
            assert_eq!(no_op.identity(db), t.identity(db));
            let direct = specialize_typevar(db, &env, t, u_context, int);
            assert_ne!(direct.identity(db), t.identity(db));
            let through_v = specialize_typevar(db, &env, t, u_context, Type::TypeVar(v));
            let staged = specialize_typevar(db, &env, through_v, v_context, int);
            assert_eq!(staged.identity(db), direct.identity(db));
            let restored = specialize_typevar(db, &env, through_v, v_context, Type::TypeVar(u));
            assert_eq!(restored.identity(db), t.identity(db));
        }
    }

    #[test]
    fn freshening_updates_ordinary_typevar_domains() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let u = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let bound = KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(u)]);
        let t = bound_typevar(
            db,
            &env,
            "T",
            TypeVarKind::Pep695TypeVar,
            Some(TypeVarBoundOrConstraints::UpperBound(bound).into()),
            TypeVarNonce::NONE,
        );
        let u_context = GenericContext::from_typevar_instances(db, &env, [u]);
        let mapping = TypeMapping::FreshenBoundTypeVars {
            generic_context: u_context,
            delta: 1,
        };
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let fresh_u = u
            .apply_type_mapping_impl(db, &mapping, &visitor)
            .as_typevar()
            .expect("freshening a typevar should return a typevar");
        let mapped_t = t
            .apply_type_mapping_impl(db, &mapping, &visitor)
            .as_typevar()
            .expect("freshening a typevar should return a typevar");
        assert_ne!(mapped_t.identity(db), t.identity(db));
        assert_eq!(
            mapped_t.identity(db),
            specialize_typevar(db, &env, t, u_context, Type::TypeVar(fresh_u)).identity(db)
        );

        let both_context = GenericContext::from_typevar_instances(db, &env, [t, u]);
        let both_mapping = TypeMapping::FreshenBoundTypeVars {
            generic_context: both_context,
            delta: 1,
        };
        let fresh_t = t
            .apply_type_mapping_impl(db, &both_mapping, &ApplyTypeMappingVisitor::new(&env))
            .as_typevar()
            .expect("freshening a typevar should return a typevar");
        // Freshening `U` changes `T`'s domain as well as its nonce. A later freshening of the
        // original context still needs to see that nonce, or it could recreate `fresh_t`.
        let max_freshness = max_typevar_freshness_matching_generic_context(
            db,
            [Type::TypeVar(fresh_t)],
            both_context,
        );
        assert_eq!(max_freshness, Some(TypeVarNonce::FIRST));
        let next_mapping = TypeMapping::FreshenBoundTypeVars {
            generic_context: both_context,
            delta: max_freshness.map_or(1, |nonce| nonce.increment().value()),
        };
        let next_t = t
            .apply_type_mapping_impl(db, &next_mapping, &ApplyTypeMappingVisitor::new(&env))
            .as_typevar()
            .expect("freshening a typevar should return a typevar");
        assert_ne!(next_t.identity(db), fresh_t.identity(db));

        let mapped_t_context = GenericContext::from_typevar_instances(db, &env, [mapped_t]);
        let fresh_mapped_t = mapped_t
            .apply_type_mapping_impl(
                db,
                &TypeMapping::FreshenBoundTypeVars {
                    generic_context: mapped_t_context,
                    delta: 1,
                },
                &ApplyTypeMappingVisitor::new(&env),
            )
            .as_typevar()
            .expect("freshening a typevar should return a typevar");
        assert_eq!(fresh_t.identity(db), fresh_mapped_t.identity(db));
    }

    #[test]
    fn freshening_captured_typevars_preserves_lazy_domains() {
        let mut db = setup_db();
        db.clear_salsa_events();
        let env = db.program_environment();
        let u = BoundTypeVarInstance::synthetic(
            &db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let context = GenericContext::from_typevar_instances(&db, &env, [u]);
        let mapping = TypeMapping::FreshenBoundTypeVars {
            generic_context: context,
            delta: 1,
        };
        for domain in [
            TypeVarBoundOrConstraintsEvaluation::LazyUpperBound,
            TypeVarBoundOrConstraintsEvaluation::LazyConstraints,
        ] {
            let captured = bound_typevar(
                &db,
                &env,
                "T",
                TypeVarKind::Pep695TypeVar,
                Some(domain),
                TypeVarNonce::NONE,
            );
            let freshened = captured.apply_type_mapping_impl(
                &db,
                &mapping,
                &ApplyTypeMappingVisitor::new(&env),
            );
            assert_eq!(freshened, Type::TypeVar(captured));
        }
        let events = db.take_salsa_events();
        assert_function_query_was_not_run_by_name(&db, "lazy_bound_unchecked", None, &events);
        assert_function_query_was_not_run_by_name(&db, "lazy_constraints_unchecked", None, &events);
    }

    #[test]
    fn specialized_typevar_preserves_its_materialized_and_transposed_views() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let u = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let context = GenericContext::from_typevar_instances(db, &env, [u]);
        let int = KnownClass::Int.to_instance(db, &env);
        let gradual_bound =
            KnownClass::Dict.to_specialized_instance(db, &env, &[Type::TypeVar(u), Type::any()]);
        let t = bound_typevar(
            db,
            &env,
            "T",
            TypeVarKind::Pep695TypeVar,
            Some(TypeVarBoundOrConstraints::UpperBound(gradual_bound).into()),
            TypeVarNonce::NONE,
        );
        let view = t.materialize_impl(
            db,
            MaterializationKind::Top,
            &ApplyTypeMappingVisitor::new(&env),
        );
        assert_eq!(view.identity(db), t.identity(db));
        assert_ne!(view.typevar(db), t.typevar(db));
        let projected = specialize_typevar(db, &env, t, context, int);
        let projected_view = specialize_typevar(db, &env, view, context, int);
        assert_eq!(projected.identity(db), projected_view.identity(db));
        assert_ne!(projected.typevar(db), projected_view.typevar(db));

        let class_bound: Type<'_> = KnownClass::List
            .to_specialized_class_type(db, &env, &[Type::TypeVar(u)])
            .expect("list should accept one type argument")
            .into();
        let class_typevar = bound_typevar(
            db,
            &env,
            "ClassT",
            TypeVarKind::Pep695TypeVar,
            Some(TypeVarBoundOrConstraints::UpperBound(class_bound).into()),
            TypeVarNonce::NONE,
        );
        let transposed = class_typevar
            .to_instance(db, &env)
            .map(InstanceProjection::into_inner)
            .expect("a class bound should have an instance projection");
        let specialized_then_transposed = specialize_typevar(db, &env, class_typevar, context, int)
            .to_instance(db, &env)
            .map(InstanceProjection::into_inner)
            .expect("a specialized class bound should have an instance projection");
        let transposed_then_specialized = specialize_typevar(db, &env, transposed, context, int);
        assert_eq!(
            specialized_then_transposed.identity(db),
            transposed_then_specialized.identity(db)
        );
        assert_eq!(
            specialized_then_transposed
                .typevar(db)
                .bound_or_constraints(db, &env),
            transposed_then_specialized
                .typevar(db)
                .bound_or_constraints(db, &env)
        );
    }

    #[test]
    fn specialization_updates_the_domain_of_a_retained_bound_view() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let u = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let original_bound =
            KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(u)]);
        let view_bound = KnownClass::Set.to_specialized_instance(db, &env, &[Type::TypeVar(u)]);
        let t = bound_typevar(
            db,
            &env,
            "T",
            TypeVarKind::Pep695TypeVar,
            Some(TypeVarBoundOrConstraints::UpperBound(original_bound).into()),
            TypeVarNonce::NONE,
        );
        let view = t.map_bound_or_constraints(db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(view_bound))
        });
        assert_eq!(view.identity(db), t.identity(db));
        let int = KnownClass::Int.to_instance(db, &env);
        let context = GenericContext::from_typevar_instances(db, &env, [t, u]);
        let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::specialization(
            context.specialize(db, vec![Type::TypeVar(view), int]),
        ));
        let result = t
            .apply_type_mapping_impl(db, &mapping, &ApplyTypeMappingVisitor::new(&env))
            .as_typevar()
            .expect("the retained typevar should remain a typevar");
        let u_context = GenericContext::from_typevar_instances(db, &env, [u]);
        let expected = specialize_typevar(db, &env, t, u_context, int);
        assert_eq!(result.identity(db), expected.identity(db));
        assert_eq!(
            result.typevar(db).bound_or_constraints(db, &env),
            Some(TypeVarBoundOrConstraints::UpperBound(
                KnownClass::Set.to_specialized_instance(db, &env, &[int])
            )),
        );
    }

    #[test]
    fn specialization_with_materialization_updates_a_retained_bound_view() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let u = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let bound =
            KnownClass::Dict.to_specialized_instance(db, &env, &[Type::TypeVar(u), Type::any()]);
        let t = bound_typevar(
            db,
            &env,
            "T",
            TypeVarKind::Pep695TypeVar,
            Some(TypeVarBoundOrConstraints::UpperBound(bound).into()),
            TypeVarNonce::NONE,
        );
        let view = t.materialize_impl(
            db,
            MaterializationKind::Top,
            &ApplyTypeMappingVisitor::new(&env),
        );
        assert_ne!(view, t);
        assert_eq!(view.identity(db), t.identity(db));

        let int = KnownClass::Int.to_instance(db, &env);
        let context = GenericContext::from_typevar_instances(db, &env, [t, u]);
        let mapping = TypeMapping::ApplySpecializationWithMaterialization {
            specialization: ApplySpecialization::specialization(
                context.specialize(db, vec![Type::TypeVar(view), int]),
            ),
            materialization_kind: MaterializationKind::Top,
        };
        let result = t
            .apply_type_mapping_impl(db, &mapping, &ApplyTypeMappingVisitor::new(&env))
            .as_typevar()
            .expect("the retained typevar should remain a typevar");
        let u_context = GenericContext::from_typevar_instances(db, &env, [u]);
        let expected = specialize_typevar(db, &env, view, u_context, int);
        assert_eq!(result.identity(db), expected.identity(db));
        assert_eq!(
            result.typevar(db).bound_or_constraints(db, &env),
            expected.typevar(db).bound_or_constraints(db, &env),
        );
    }

    #[test]
    fn self_domain_projection_identity() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let u = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let t_context = GenericContext::from_typevar_instances(db, &env, [t]);
        let u_context = GenericContext::from_typevar_instances(db, &env, [u]);
        let domain = KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(t)]);
        let self_typevar = BoundTypeVarInstance::synthetic_self(
            db,
            domain,
            BindingContext::Synthetic(env.program(db)),
        );
        let int = KnownClass::Int.to_instance(db, &env);
        let str = KnownClass::Str.to_instance(db, &env);
        let mapping = |context, ty| specialization_mapping(db, context, ty);
        let projected_int = map_self(db, &env, self_typevar, &mapping(t_context, int));
        let constructed_int = BoundTypeVarInstance::synthetic_self(
            db,
            KnownClass::List.to_specialized_instance(db, &env, &[int]),
            BindingContext::Synthetic(env.program(db)),
        );
        assert_eq!(projected_int.identity(db), constructed_int.identity(db));

        assert_eq!(
            map_self(
                db,
                &env,
                self_typevar,
                &mapping(t_context, Type::TypeVar(t))
            )
            .identity(db),
            self_typevar.identity(db)
        );
        assert_eq!(
            projected_int.identity(db),
            map_self(db, &env, self_typevar, &mapping(t_context, int)).identity(db)
        );
        assert_ne!(
            projected_int.identity(db),
            map_self(db, &env, self_typevar, &mapping(t_context, str)).identity(db)
        );
        let staged = map_self(
            db,
            &env,
            map_self(
                db,
                &env,
                self_typevar,
                &mapping(t_context, Type::TypeVar(u)),
            ),
            &mapping(u_context, int),
        );
        assert_eq!(projected_int.identity(db), staged.identity(db));

        let context = GenericContext::from_typevar_instances(db, &env, [t, self_typevar]);
        let mapping_with_retained_self =
            TypeMapping::ApplySpecialization(ApplySpecialization::Specialization {
                specialization: context.specialize(db, vec![int, Type::TypeVar(self_typevar)]),
            });
        let retained_self = self_typevar
            .apply_type_mapping_impl(
                db,
                &mapping_with_retained_self,
                &ApplyTypeMappingVisitor::new(&env),
            )
            .as_typevar()
            .expect("the retained Self should remain a typevar");
        assert_eq!(retained_self.identity(db), projected_int.identity(db));

        // Another view of the bound shares the binder identity, even after owner specialization.
        let view = self_typevar.map_bound_or_constraints(db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(Type::object()))
        });
        assert_eq!(view.identity(db), self_typevar.identity(db));
        let projected_view = map_self(db, &env, view, &mapping(t_context, int));
        assert_eq!(projected_view.identity(db), projected_int.identity(db));
        assert_eq!(
            projected_view.typevar(db).bound_or_constraints(db, &env),
            Some(TypeVarBoundOrConstraints::UpperBound(Type::object()))
        );
        assert_ne!(projected_view.typevar(db), projected_int.typevar(db));
    }

    #[test]
    fn owned_self_freshening_commutes_with_projection() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let domain = KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(t)]);
        let self_typevar = BoundTypeVarInstance::synthetic_self(
            db,
            domain,
            BindingContext::Synthetic(env.program(db)),
        );
        let context = GenericContext::from_typevar_instances(db, &env, [self_typevar, t]);
        let freshen_mapping = TypeMapping::FreshenBoundTypeVars {
            generic_context: context,
            delta: 1,
        };
        let visitor = ApplyTypeMappingVisitor::new(&env);
        let fresh_t = t
            .apply_type_mapping_impl(db, &freshen_mapping, &visitor)
            .as_typevar()
            .expect("freshening a typevar should return a typevar");
        let fresh_self = self_typevar
            .apply_type_mapping_impl(db, &freshen_mapping, &visitor)
            .as_typevar()
            .expect("freshening a typevar should return a typevar");
        let int = KnownClass::Int.to_instance(db, &env);
        let t_context = GenericContext::from_typevar_instances(db, &env, [t]);
        let fresh_t_context = GenericContext::from_typevar_instances(db, &env, [fresh_t]);
        let mapping = |context| specialization_mapping(db, context, int);
        let projected = map_self(db, &env, self_typevar, &mapping(t_context));
        let projected_context = GenericContext::from_typevar_instances(db, &env, [projected, t]);
        let projected_freshen = TypeMapping::FreshenBoundTypeVars {
            generic_context: projected_context,
            delta: 1,
        };
        let fresh_projected = projected
            .apply_type_mapping_impl(db, &projected_freshen, &visitor)
            .as_typevar()
            .expect("freshening a typevar should return a typevar");
        assert_ne!(fresh_self.identity(db), self_typevar.identity(db));
        assert_eq!(
            map_self(db, &env, fresh_self, &mapping(fresh_t_context)).identity(db),
            fresh_projected.identity(db)
        );
    }

    #[test]
    fn materialized_self_domain_keeps_its_canonical_identity() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        );
        let domain =
            KnownClass::Dict.to_specialized_instance(db, &env, &[Type::TypeVar(t), Type::any()]);
        let self_typevar = BoundTypeVarInstance::synthetic_self(
            db,
            domain,
            BindingContext::Synthetic(env.program(db)),
        );
        let materialized = self_typevar.materialize_impl(
            db,
            MaterializationKind::Top,
            &ApplyTypeMappingVisitor::new(&env),
        );
        assert_ne!(materialized.typevar(db), self_typevar.typevar(db));
        assert_eq!(materialized.identity(db), self_typevar.identity(db));

        let context = GenericContext::from_typevar_instances(db, &env, [t]);
        let mapping = TypeMapping::ApplySpecialization(ApplySpecialization::specialization(
            context.specialize(db, vec![KnownClass::Int.to_instance(db, &env)]),
        ));
        let projected = map_self(db, &env, self_typevar, &mapping);
        let projected_view = map_self(db, &env, materialized, &mapping);
        assert_eq!(projected.identity(db), projected_view.identity(db));
        assert_ne!(projected.typevar(db), projected_view.typevar(db));

        let gradual_specialization =
            ApplySpecialization::specialization(context.specialize(db, vec![Type::any()]));
        let plain = map_self(
            db,
            &env,
            self_typevar,
            &TypeMapping::ApplySpecialization(gradual_specialization),
        );
        let materializing = map_self(
            db,
            &env,
            self_typevar,
            &TypeMapping::ApplySpecializationWithMaterialization {
                specialization: gradual_specialization,
                materialization_kind: MaterializationKind::Top,
            },
        );
        assert_eq!(plain.identity(db), materializing.identity(db));
        assert_ne!(plain.typevar(db), materializing.typevar(db));
    }

    #[test]
    fn transposed_self_domain_keeps_its_canonical_identity() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let self_typevar = BoundTypeVarInstance::synthetic_self(
            db,
            KnownClass::Int.to_class_literal(db, &env),
            BindingContext::Synthetic(env.program(db)),
        );
        let view = self_typevar.map_bound_or_constraints(db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(
                KnownClass::Str.to_class_literal(db, &env),
            ))
        });
        let transposed = self_typevar
            .to_instance(db, &env)
            .map(InstanceProjection::into_inner);
        let transposed_view = view
            .to_instance(db, &env)
            .map(InstanceProjection::into_inner);
        assert!(transposed.is_some());
        assert!(transposed_view.is_some());
        assert_eq!(
            transposed.map(|ty| ty.identity(db)),
            transposed_view.map(|ty| ty.identity(db))
        );
        assert_ne!(transposed, transposed_view);
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

        let context = GenericContext::from_typevar_instances(&db, &env, [u]);
        let lazy_after_no_op = specialize_typevar(&db, &env, lazy, context, Type::TypeVar(v));
        let eager_after_no_op = specialize_typevar(&db, &env, eager, context, Type::TypeVar(v));
        assert_eq!(
            lazy_after_no_op.identity(&db),
            eager_after_no_op.identity(&db)
        );
        assert_eq!(lazy_after_no_op.identity(&db), lazy.identity(&db));

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
