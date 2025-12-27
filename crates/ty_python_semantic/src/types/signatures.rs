//! _Signatures_ describe the expected parameters and return type of a function or other callable.
//! Overloads and unions add complexity to this simple description.
//!
//! In a call expression, the type of the callable might be a union of several types. The call must
//! be compatible with _all_ of these types, since at runtime the callable might be an instance of
//! any of them.
//!
//! Each of the atomic types in the union must be callable. Each callable might be _overloaded_,
//! containing multiple _overload signatures_, each of which describes a different combination of
//! argument types and return types. For each callable type in the union, the call expression's
//! arguments must match _at least one_ overload.

use std::slice::Iter;

use itertools::{EitherOrBoth, Itertools};
use rustc_hash::FxHashMap;
use smallvec::{SmallVec, smallvec_inline};

use super::{DynamicType, Type, TypeVarVariance, definition_expression_type, semantic_index};
use crate::semantic_index::definition::Definition;
use crate::types::constraints::{
    ConstraintSet, IteratorConstraintsExtension, OptionConstraintsExtension,
};
use crate::types::generics::{GenericContext, InferableTypeVars, walk_generic_context};
use crate::types::infer::{infer_deferred_types, infer_scope_types};
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance, CallableType, CallableTypeKind,
    FindLegacyTypeVarsVisitor, HasRelationToVisitor, IsDisjointVisitor, IsEquivalentVisitor,
    KnownClass, MaterializationKind, NormalizedVisitor, ParamSpecAttrKind, TypeContext,
    TypeMapping, TypeRelation, VarianceInferable, todo_type,
};
use crate::{Db, FxOrderSet};
use ruff_python_ast::{self as ast, name::Name};

/// Infer the type of a parameter or return annotation in a function signature.
///
/// This is very similar to [`definition_expression_type`], but knows that `TypeInferenceBuilder`
/// will always infer the parameters and return of a function in its PEP-695 typevar scope, if
/// there is one; otherwise they will be inferred in the function definition scope, but will always
/// be deferred. (This prevents spurious salsa cycles when we need the signature of the function
/// while in the middle of inferring its definition scope — for instance, when applying
/// decorators.)
fn function_signature_expression_type<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
) -> Type<'db> {
    let file = definition.file(db);
    let index = semantic_index(db, file);
    let file_scope = index.expression_scope_id(expression);
    let scope = file_scope.to_scope_id(db, file);
    if scope == definition.scope(db) {
        // expression is in the function definition scope, but always deferred
        infer_deferred_types(db, definition).expression_type(expression)
    } else {
        // expression is in the PEP-695 type params sub-scope
        infer_scope_types(db, scope).expression_type(expression)
    }
}

/// The signature of a single callable. If the callable is overloaded, there is a separate
/// [`Signature`] for each overload.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub struct CallableSignature<'db> {
    /// The signatures of each overload of this callable. Will be empty if the type is not
    /// callable.
    pub(crate) overloads: SmallVec<[Signature<'db>; 1]>,
}

impl<'db> CallableSignature<'db> {
    pub(crate) fn single(signature: Signature<'db>) -> Self {
        Self {
            overloads: smallvec_inline![signature],
        }
    }

    pub(crate) fn bottom() -> Self {
        Self::single(Signature::bottom())
    }

    /// Creates a new `CallableSignature` from an iterator of [`Signature`]s. Returns a
    /// non-callable signature if the iterator is empty.
    pub(crate) fn from_overloads<I>(overloads: I) -> Self
    where
        I: IntoIterator<Item = Signature<'db>>,
    {
        Self {
            overloads: overloads.into_iter().collect(),
        }
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Signature<'db>> {
        self.overloads.iter()
    }

    pub(crate) fn with_inherited_generic_context(
        &self,
        db: &'db dyn Db,
        inherited_generic_context: GenericContext<'db>,
    ) -> Self {
        Self::from_overloads(self.overloads.iter().map(|signature| {
            signature
                .clone()
                .with_inherited_generic_context(db, inherited_generic_context)
        }))
    }

    pub(crate) fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Self {
        Self::from_overloads(
            self.overloads
                .iter()
                .map(|signature| signature.normalized_impl(db, visitor)),
        )
    }

    pub(super) fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        Some(Self {
            overloads: self
                .overloads
                .iter()
                .map(|signature| signature.recursive_type_normalized_impl(db, div, nested))
                .collect::<Option<SmallVec<_>>>()?,
        })
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        fn try_apply_type_mapping_for_paramspec<'db>(
            db: &'db dyn Db,
            self_signature: &Signature<'db>,
            prefix_parameters: &[Parameter<'db>],
            paramspec_value: Type<'db>,
            type_mapping: &TypeMapping<'_, 'db>,
            tcx: TypeContext<'db>,
            visitor: &ApplyTypeMappingVisitor<'db>,
        ) -> Option<CallableSignature<'db>> {
            match paramspec_value {
                Type::TypeVar(typevar) if typevar.is_paramspec(db) => {
                    Some(CallableSignature::single(Signature {
                        generic_context: self_signature.generic_context.map(|context| {
                            type_mapping.update_signature_generic_context(db, context)
                        }),
                        definition: self_signature.definition,
                        parameters: Parameters::new(
                            db,
                            prefix_parameters
                                .iter()
                                .map(|param| {
                                    param.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                                })
                                .chain([
                                    Parameter::variadic(Name::new_static("args"))
                                        .with_annotated_type(Type::TypeVar(
                                            typevar
                                                .with_paramspec_attr(db, ParamSpecAttrKind::Args),
                                        )),
                                    Parameter::keyword_variadic(Name::new_static("kwargs"))
                                        .with_annotated_type(Type::TypeVar(
                                            typevar
                                                .with_paramspec_attr(db, ParamSpecAttrKind::Kwargs),
                                        )),
                                ]),
                        ),
                        return_ty: self_signature
                            .return_ty
                            .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
                    }))
                }
                Type::Callable(callable)
                    if matches!(callable.kind(db), CallableTypeKind::ParamSpecValue) =>
                {
                    Some(CallableSignature::from_overloads(
                        callable.signatures(db).iter().map(|signature| Signature {
                            generic_context: self_signature.generic_context.map(|context| {
                                type_mapping.update_signature_generic_context(db, context)
                            }),
                            definition: signature.definition,
                            parameters: if signature.parameters().is_top() {
                                signature.parameters().clone()
                            } else {
                                Parameters::new(
                                    db,
                                    prefix_parameters
                                        .iter()
                                        .map(|param| {
                                            param.apply_type_mapping_impl(
                                                db,
                                                type_mapping,
                                                tcx,
                                                visitor,
                                            )
                                        })
                                        .chain(signature.parameters().iter().cloned()),
                                )
                            },
                            return_ty: self_signature.return_ty.map(|ty| {
                                ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                            }),
                        }),
                    ))
                }
                _ => None,
            }
        }

        match type_mapping {
            TypeMapping::Specialization(specialization) => {
                if let [self_signature] = self.overloads.as_slice()
                    && let Some((prefix_parameters, paramspec)) = self_signature
                        .parameters
                        .find_paramspec_from_args_kwargs(db)
                    && let Some(paramspec_value) = specialization.get(db, paramspec)
                    && let Some(result) = try_apply_type_mapping_for_paramspec(
                        db,
                        self_signature,
                        prefix_parameters,
                        paramspec_value,
                        type_mapping,
                        tcx,
                        visitor,
                    )
                {
                    return result;
                }
            }
            TypeMapping::PartialSpecialization(partial) => {
                if let [self_signature] = self.overloads.as_slice()
                    && let Some((prefix_parameters, paramspec)) = self_signature
                        .parameters
                        .find_paramspec_from_args_kwargs(db)
                    && let Some(paramspec_value) = partial.get(db, paramspec)
                    && let Some(result) = try_apply_type_mapping_for_paramspec(
                        db,
                        self_signature,
                        prefix_parameters,
                        paramspec_value,
                        type_mapping,
                        tcx,
                        visitor,
                    )
                {
                    return result;
                }
            }
            _ => {}
        }

        Self::from_overloads(
            self.overloads
                .iter()
                .map(|signature| signature.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
        )
    }

    pub(crate) fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for signature in &self.overloads {
            signature.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
    }

    /// Binds the first (presumably `self`) parameter of this signature. If a `self_type` is
    /// provided, we will replace any occurrences of `typing.Self` in the parameter and return
    /// annotations with that type.
    pub(crate) fn bind_self(&self, db: &'db dyn Db, self_type: Option<Type<'db>>) -> Self {
        Self {
            overloads: self
                .overloads
                .iter()
                .map(|signature| signature.bind_self(db, self_type))
                .collect(),
        }
    }

    /// Replaces any occurrences of `typing.Self` in the parameter and return annotations with the
    /// given type. (Does not bind the `self` parameter; to do that, use
    /// [`bind_self`][Self::bind_self].)
    pub(crate) fn apply_self(&self, db: &'db dyn Db, self_type: Type<'db>) -> Self {
        Self {
            overloads: self
                .overloads
                .iter()
                .map(|signature| signature.apply_self(db, self_type))
                .collect(),
        }
    }

    fn is_subtype_of_impl(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to_impl(
            db,
            other,
            inferable,
            TypeRelation::Subtyping,
            &HasRelationToVisitor::default(),
            &IsDisjointVisitor::default(),
        )
    }

    pub(crate) fn has_relation_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        Self::has_relation_to_inner(
            db,
            &self.overloads,
            &other.overloads,
            inferable,
            relation,
            relation_visitor,
            disjointness_visitor,
        )
    }

    pub(crate) fn is_single_paramspec(
        &self,
    ) -> Option<(BoundTypeVarInstance<'db>, Option<Type<'db>>)> {
        Self::signatures_is_single_paramspec(&self.overloads)
    }

    /// Checks whether the given slice contains a single signature, and that signature is a
    /// `ParamSpec` signature. If so, returns the [`BoundTypeVarInstance`] for the `ParamSpec`,
    /// along with the return type of the signature.
    fn signatures_is_single_paramspec(
        signatures: &[Signature<'db>],
    ) -> Option<(BoundTypeVarInstance<'db>, Option<Type<'db>>)> {
        // TODO: This might need updating once we support `Concatenate`
        let [signature] = signatures else {
            return None;
        };
        signature
            .parameters
            .as_paramspec()
            .map(|bound_typevar| (bound_typevar, signature.return_ty))
    }

    pub(crate) fn when_constraint_set_assignable_to(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to_impl(
            db,
            other,
            inferable,
            TypeRelation::ConstraintSetAssignability,
            &HasRelationToVisitor::default(),
            &IsDisjointVisitor::default(),
        )
    }

    /// Implementation of subtyping and assignability between two, possible overloaded, callable
    /// types.
    fn has_relation_to_inner(
        db: &'db dyn Db,
        self_signatures: &[Signature<'db>],
        other_signatures: &[Signature<'db>],
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if relation.is_constraint_set_assignability() {
            // TODO: Oof, maybe ParamSpec needs to live at CallableSignature, not Signature?
            let self_is_single_paramspec = Self::signatures_is_single_paramspec(self_signatures);
            let other_is_single_paramspec = Self::signatures_is_single_paramspec(other_signatures);

            // If either callable is a ParamSpec, the constraint set should bind the ParamSpec to
            // the other callable's signature. We also need to compare the return types — for
            // instance, to verify in `Callable[P, int]` that the return type is assignable to
            // `int`, or in `Callable[P, T]` to bind `T` to the return type of the other callable.
            match (self_is_single_paramspec, other_is_single_paramspec) {
                (
                    Some((self_bound_typevar, self_return_type)),
                    Some((other_bound_typevar, other_return_type)),
                ) => {
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self_bound_typevar,
                        Type::TypeVar(other_bound_typevar),
                        Type::TypeVar(other_bound_typevar),
                    );
                    let return_types_match = self_return_type.zip(other_return_type).when_some_and(
                        |(self_return_type, other_return_type)| {
                            self_return_type.has_relation_to_impl(
                                db,
                                other_return_type,
                                inferable,
                                relation,
                                relation_visitor,
                                disjointness_visitor,
                            )
                        },
                    );
                    return param_spec_matches.and(db, || return_types_match);
                }

                (Some((self_bound_typevar, self_return_type)), None) => {
                    let upper =
                        Type::Callable(CallableType::new(
                            db,
                            CallableSignature::from_overloads(other_signatures.iter().map(
                                |signature| Signature::new(signature.parameters().clone(), None),
                            )),
                            CallableTypeKind::ParamSpecValue,
                        ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self_bound_typevar,
                        Type::Never,
                        upper,
                    );
                    let return_types_match = self_return_type.when_some_and(|self_return_type| {
                        other_signatures
                            .iter()
                            .filter_map(|signature| signature.return_ty)
                            .when_any(db, |other_return_type| {
                                self_return_type.has_relation_to_impl(
                                    db,
                                    other_return_type,
                                    inferable,
                                    relation,
                                    relation_visitor,
                                    disjointness_visitor,
                                )
                            })
                    });
                    return param_spec_matches.and(db, || return_types_match);
                }

                (None, Some((other_bound_typevar, other_return_type))) => {
                    let lower =
                        Type::Callable(CallableType::new(
                            db,
                            CallableSignature::from_overloads(self_signatures.iter().map(
                                |signature| Signature::new(signature.parameters().clone(), None),
                            )),
                            CallableTypeKind::ParamSpecValue,
                        ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        other_bound_typevar,
                        lower,
                        Type::object(),
                    );
                    let return_types_match = other_return_type.when_some_and(|other_return_type| {
                        self_signatures
                            .iter()
                            .filter_map(|signature| signature.return_ty)
                            .when_any(db, |self_return_type| {
                                self_return_type.has_relation_to_impl(
                                    db,
                                    other_return_type,
                                    inferable,
                                    relation,
                                    relation_visitor,
                                    disjointness_visitor,
                                )
                            })
                    });
                    return param_spec_matches.and(db, || return_types_match);
                }

                (None, None) => {}
            }
        }

        match (self_signatures, other_signatures) {
            ([self_signature], [other_signature]) => {
                // Base case: both callable types contain a single signature.
                self_signature.has_relation_to_impl(
                    db,
                    other_signature,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }

            // `self` is possibly overloaded while `other` is definitely not overloaded.
            (_, [_]) => self_signatures.iter().when_any(db, |self_signature| {
                Self::has_relation_to_inner(
                    db,
                    std::slice::from_ref(self_signature),
                    other_signatures,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),

            // `self` is definitely not overloaded while `other` is possibly overloaded.
            ([_], _) => other_signatures.iter().when_all(db, |other_signature| {
                Self::has_relation_to_inner(
                    db,
                    self_signatures,
                    std::slice::from_ref(other_signature),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),

            // `self` is definitely overloaded while `other` is possibly overloaded.
            (_, _) => other_signatures.iter().when_all(db, |other_signature| {
                Self::has_relation_to_inner(
                    db,
                    self_signatures,
                    std::slice::from_ref(other_signature),
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),
        }
    }

    /// Check whether this callable type is equivalent to another callable type.
    ///
    /// See [`Type::is_equivalent_to`] for more details.
    pub(crate) fn is_equivalent_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        match (self.overloads.as_slice(), other.overloads.as_slice()) {
            ([self_signature], [other_signature]) => {
                // Common case: both callable types contain a single signature, use the custom
                // equivalence check instead of delegating it to the subtype check.
                self_signature.is_equivalent_to_impl(db, other_signature, inferable, visitor)
            }
            (_, _) => {
                if self == other {
                    return ConstraintSet::from(true);
                }
                self.is_subtype_of_impl(db, other, inferable)
                    .and(db, || other.is_subtype_of_impl(db, self, inferable))
            }
        }
    }
}

impl<'a, 'db> IntoIterator for &'a CallableSignature<'db> {
    type Item = &'a Signature<'db>;
    type IntoIter = std::slice::Iter<'a, Signature<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'db> VarianceInferable<'db> for &CallableSignature<'db> {
    // TODO: possibly need to replace self
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        self.overloads
            .iter()
            .map(|signature| signature.variance_of(db, typevar))
            .collect()
    }
}

/// The signature of one of the overloads of a callable.
#[derive(Clone, Debug, salsa::Update, get_size2::GetSize, PartialEq, Eq, Hash)]
pub struct Signature<'db> {
    /// The generic context for this overload, if it is generic.
    pub(crate) generic_context: Option<GenericContext<'db>>,

    /// The original definition associated with this function, if available.
    /// This is useful for locating and extracting docstring information for the signature.
    pub(crate) definition: Option<Definition<'db>>,

    /// Parameters, in source order.
    ///
    /// The ordering of parameters in a valid signature must be: first positional-only parameters,
    /// then positional-or-keyword, then optionally the variadic parameter, then keyword-only
    /// parameters, and last, optionally the variadic keywords parameter. Parameters with defaults
    /// must come after parameters without defaults.
    ///
    /// We may get invalid signatures, though, and need to handle them without panicking.
    parameters: Parameters<'db>,

    /// Annotated return type, if any.
    pub(crate) return_ty: Option<Type<'db>>,
}

pub(super) fn walk_signature<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    signature: &Signature<'db>,
    visitor: &V,
) {
    if let Some(generic_context) = &signature.generic_context {
        walk_generic_context(db, *generic_context, visitor);
    }
    // By default we usually don't visit the type of the default value,
    // as it isn't relevant to most things
    for parameter in &signature.parameters {
        if let Some(ty) = parameter.annotated_type() {
            visitor.visit_type(db, ty);
        }
    }
    if let Some(return_ty) = &signature.return_ty {
        visitor.visit_type(db, *return_ty);
    }
}

impl<'db> Signature<'db> {
    pub(crate) fn new(parameters: Parameters<'db>, return_ty: Option<Type<'db>>) -> Self {
        Self {
            generic_context: None,
            definition: None,
            parameters,
            return_ty,
        }
    }

    pub(crate) fn new_generic(
        generic_context: Option<GenericContext<'db>>,
        parameters: Parameters<'db>,
        return_ty: Option<Type<'db>>,
    ) -> Self {
        Self {
            generic_context,
            definition: None,
            parameters,
            return_ty,
        }
    }

    /// Return a signature for a dynamic callable
    pub(crate) fn dynamic(signature_type: Type<'db>) -> Self {
        Signature {
            generic_context: None,
            definition: None,
            parameters: Parameters::gradual_form(),
            return_ty: Some(signature_type),
        }
    }

    /// Return a todo signature: (*args: Todo, **kwargs: Todo) -> Todo
    #[allow(unused_variables)] // 'reason' only unused in debug builds
    pub(crate) fn todo(reason: &'static str) -> Self {
        let signature_type = todo_type!(reason);
        Signature {
            generic_context: None,
            definition: None,
            parameters: Parameters::todo(),
            return_ty: Some(signature_type),
        }
    }

    /// Return a typed signature from a function definition.
    pub(super) fn from_function(
        db: &'db dyn Db,
        pep695_generic_context: Option<GenericContext<'db>>,
        definition: Definition<'db>,
        function_node: &ast::StmtFunctionDef,
        has_implicitly_positional_first_parameter: bool,
    ) -> Self {
        let parameters = Parameters::from_parameters(
            db,
            definition,
            function_node.parameters.as_ref(),
            has_implicitly_positional_first_parameter,
        );
        let return_ty = function_node
            .returns
            .as_ref()
            .map(|returns| function_signature_expression_type(db, definition, returns.as_ref()));
        let legacy_generic_context =
            GenericContext::from_function_params(db, definition, &parameters, return_ty);
        let full_generic_context = GenericContext::merge_pep695_and_legacy(
            db,
            pep695_generic_context,
            legacy_generic_context,
        );

        Self {
            generic_context: full_generic_context,
            definition: Some(definition),
            parameters,
            return_ty,
        }
    }

    pub(super) fn wrap_coroutine_return_type(self, db: &'db dyn Db) -> Self {
        let return_ty = self.return_ty.map(|return_ty| {
            KnownClass::CoroutineType
                .to_specialized_instance(db, [Type::any(), Type::any(), return_ty])
        });
        Self { return_ty, ..self }
    }

    /// Returns the signature which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown() -> Self {
        Self::new(Parameters::unknown(), Some(Type::unknown()))
    }

    /// Return the "bottom" signature, subtype of all other fully-static signatures.
    pub(crate) fn bottom() -> Self {
        Self::new(Parameters::bottom(), Some(Type::Never))
    }

    pub(crate) fn with_inherited_generic_context(
        mut self,
        db: &'db dyn Db,
        inherited_generic_context: GenericContext<'db>,
    ) -> Self {
        match self.generic_context.as_mut() {
            Some(generic_context) => {
                *generic_context = generic_context.merge(db, inherited_generic_context);
            }
            None => {
                self.generic_context = Some(inherited_generic_context);
            }
        }
        self
    }

    pub(crate) fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Self {
        Self {
            generic_context: self
                .generic_context
                .map(|ctx| ctx.normalized_impl(db, visitor)),
            // Discard the definition when normalizing, so that two equivalent signatures
            // with different `Definition`s share the same Salsa ID when normalized
            definition: None,
            parameters: Parameters::new(
                db,
                self.parameters
                    .iter()
                    .map(|param| param.normalized_impl(db, visitor)),
            ),
            return_ty: self
                .return_ty
                .map(|return_ty| return_ty.normalized_impl(db, visitor)),
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let return_ty = match self.return_ty {
            Some(return_ty) if nested => {
                Some(return_ty.recursive_type_normalized_impl(db, div, true)?)
            }
            Some(return_ty) => Some(
                return_ty
                    .recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };
        let parameters = {
            let mut parameters = Vec::with_capacity(self.parameters.len());
            for param in &self.parameters {
                parameters.push(param.recursive_type_normalized_impl(db, div, nested)?);
            }
            Parameters::new(db, parameters)
        };
        Some(Self {
            generic_context: self.generic_context,
            definition: self.definition,
            parameters,
            return_ty,
        })
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self {
            generic_context: self
                .generic_context
                .map(|context| type_mapping.update_signature_generic_context(db, context)),
            definition: self.definition,
            parameters: self
                .parameters
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            return_ty: self
                .return_ty
                .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
        }
    }

    pub(crate) fn find_legacy_typevars_impl(
        &self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for param in &self.parameters {
            if let Some(ty) = param.annotated_type() {
                ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
            if let Some(ty) = param.default_type() {
                ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }
        if let Some(ty) = self.return_ty {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
    }

    /// Return the parameters in this signature.
    pub(crate) fn parameters(&self) -> &Parameters<'db> {
        &self.parameters
    }

    /// Adds an implicit annotation to the first parameter of this signature, if that parameter is
    /// positional and does not already have an annotation. We do not check whether that's the
    /// right thing to do! The caller must determine whether the first parameter is actually a
    /// `self` or `cls` parameter, and must determine the correct type to use as the implicit
    /// annotation.
    pub(crate) fn add_implicit_self_annotation(
        &mut self,
        db: &'db dyn Db,
        self_type: impl FnOnce() -> Option<Type<'db>>,
    ) {
        if let Some(first_parameter) = self.parameters.value.first_mut()
            && first_parameter.is_positional()
            && first_parameter.annotated_type.is_none()
            && let Some(self_type) = self_type()
        {
            first_parameter.annotated_type = Some(self_type);
            first_parameter.inferred_annotation = true;

            // If we've added an implicit `self` annotation, we might need to update the
            // signature's generic context, too. (The generic context should include any synthetic
            // typevars created for `typing.Self`, even if the `typing.Self` annotation was added
            // implicitly.)
            let self_typevar = match self_type {
                Type::TypeVar(self_typevar) => Some(self_typevar),
                Type::SubclassOf(subclass_of) => subclass_of.into_type_var(),
                _ => None,
            };

            if let Some(self_typevar) = self_typevar {
                match self.generic_context.as_mut() {
                    Some(generic_context)
                        if generic_context
                            .binds_typevar(db, self_typevar.typevar(db))
                            .is_some() => {}
                    Some(generic_context) => {
                        *generic_context = GenericContext::from_typevar_instances(
                            db,
                            std::iter::once(self_typevar).chain(generic_context.variables(db)),
                        );
                    }
                    None => {
                        self.generic_context = Some(GenericContext::from_typevar_instances(
                            db,
                            std::iter::once(self_typevar),
                        ));
                    }
                }
            }
        }
    }

    /// Return the definition associated with this signature, if any.
    pub(crate) fn definition(&self) -> Option<Definition<'db>> {
        self.definition
    }

    pub(crate) fn bind_self(&self, db: &'db dyn Db, self_type: Option<Type<'db>>) -> Self {
        let mut parameters = self.parameters.iter().cloned().peekable();

        // TODO: Theoretically, for a signature like `f(*args: *tuple[MyClass, int, *tuple[str, ...]])` with
        // a variadic first parameter, we should also "skip the first parameter" by modifying the tuple type.
        if parameters.peek().is_some_and(Parameter::is_positional) {
            parameters.next();
        }

        let mut parameters = Parameters::new(db, parameters);
        let mut return_ty = self.return_ty;
        let binding_context = self.definition.map(BindingContext::Definition);
        if let Some(self_type) = self_type {
            let self_mapping = TypeMapping::BindSelf {
                self_type,
                binding_context,
            };
            parameters = parameters.apply_type_mapping_impl(
                db,
                &self_mapping,
                TypeContext::default(),
                &ApplyTypeMappingVisitor::default(),
            );
            return_ty = return_ty
                .map(|ty| ty.apply_type_mapping(db, &self_mapping, TypeContext::default()));
        }
        Self {
            generic_context: self
                .generic_context
                .map(|generic_context| generic_context.remove_self(db, binding_context)),
            definition: self.definition,
            parameters,
            return_ty,
        }
    }

    pub(crate) fn apply_self(&self, db: &'db dyn Db, self_type: Type<'db>) -> Self {
        let self_mapping = TypeMapping::BindSelf {
            self_type,
            binding_context: self.definition.map(BindingContext::Definition),
        };
        let parameters = self.parameters.apply_type_mapping_impl(
            db,
            &self_mapping,
            TypeContext::default(),
            &ApplyTypeMappingVisitor::default(),
        );
        let return_ty = self
            .return_ty
            .map(|ty| ty.apply_type_mapping(db, &self_mapping, TypeContext::default()));
        Self {
            generic_context: self.generic_context,
            definition: self.definition,
            parameters,
            return_ty,
        }
    }

    fn inferable_typevars(&self, db: &'db dyn Db) -> InferableTypeVars<'db, 'db> {
        match self.generic_context {
            Some(generic_context) => generic_context.inferable_typevars(db),
            None => InferableTypeVars::None,
        }
    }

    /// Return `true` if `self` has exactly the same set of possible static materializations as
    /// `other` (if `self` represents the same set of possible sets of possible runtime objects as
    /// `other`).
    pub(crate) fn is_equivalent_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Signature<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        // If either signature is generic, their typevars should also be considered inferable when
        // checking whether the signatures are equivalent, since we only need to find one
        // specialization that causes the check to succeed.
        //
        // TODO: We should alpha-rename these typevars, too, to correctly handle when a generic
        // callable refers to typevars from within the context that defines them. This primarily
        // comes up when referring to a generic function recursively from within its body:
        //
        //     def identity[T](t: T) -> T:
        //         # Here, TypeOf[identity2] is a generic callable that should consider T to be
        //         # inferable, even though other uses of T in the function body are non-inferable.
        //         return t
        let self_inferable = self.inferable_typevars(db);
        let other_inferable = other.inferable_typevars(db);
        let inferable = inferable.merge(&self_inferable);
        let inferable = inferable.merge(&other_inferable);

        // `inner` will create a constraint set that references these newly inferable typevars.
        let when = self.is_equivalent_to_inner(db, other, inferable, visitor);

        // But the caller does not need to consider those extra typevars. Whatever constraint set
        // we produce, we reduce it back down to the inferable set that the caller asked about.
        // If we introduced new inferable typevars, those will be existentially quantified away
        // before returning.
        when.reduce_inferable(db, self_inferable.iter().chain(other_inferable.iter()))
    }

    fn is_equivalent_to_inner(
        &self,
        db: &'db dyn Db,
        other: &Signature<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        let mut result = ConstraintSet::from(true);
        let mut check_types = |self_type: Option<Type<'db>>, other_type: Option<Type<'db>>| {
            let self_type = self_type.unwrap_or(Type::unknown());
            let other_type = other_type.unwrap_or(Type::unknown());
            !result
                .intersect(
                    db,
                    self_type.is_equivalent_to_impl(db, other_type, inferable, visitor),
                )
                .is_never_satisfied(db)
        };

        if self.parameters.is_gradual() != other.parameters.is_gradual() {
            return ConstraintSet::from(false);
        }

        if self.parameters.len() != other.parameters.len() {
            return ConstraintSet::from(false);
        }

        if !check_types(self.return_ty, other.return_ty) {
            return result;
        }

        for (self_parameter, other_parameter) in self.parameters.iter().zip(&other.parameters) {
            match (self_parameter.kind(), other_parameter.kind()) {
                (
                    ParameterKind::PositionalOnly {
                        default_type: self_default,
                        ..
                    },
                    ParameterKind::PositionalOnly {
                        default_type: other_default,
                        ..
                    },
                ) if self_default.is_some() == other_default.is_some() => {}

                (
                    ParameterKind::PositionalOrKeyword {
                        name: self_name,
                        default_type: self_default,
                    },
                    ParameterKind::PositionalOrKeyword {
                        name: other_name,
                        default_type: other_default,
                    },
                ) if self_default.is_some() == other_default.is_some()
                    && self_name == other_name => {}

                (ParameterKind::Variadic { .. }, ParameterKind::Variadic { .. }) => {}

                (
                    ParameterKind::KeywordOnly {
                        name: self_name,
                        default_type: self_default,
                    },
                    ParameterKind::KeywordOnly {
                        name: other_name,
                        default_type: other_default,
                    },
                ) if self_default.is_some() == other_default.is_some()
                    && self_name == other_name => {}

                (ParameterKind::KeywordVariadic { .. }, ParameterKind::KeywordVariadic { .. }) => {}

                _ => return ConstraintSet::from(false),
            }

            if !check_types(
                self_parameter.annotated_type(),
                other_parameter.annotated_type(),
            ) {
                return result;
            }
        }

        result
    }

    pub(crate) fn when_constraint_set_assignable_to_signatures(
        &self,
        db: &'db dyn Db,
        other: &CallableSignature<'db>,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        // If this signature is a paramspec, bind it to the entire overloaded other callable.
        if let Some(self_bound_typevar) = self.parameters.as_paramspec()
            && other.is_single_paramspec().is_none()
        {
            let upper = Type::Callable(CallableType::new(
                db,
                CallableSignature::from_overloads(
                    other
                        .overloads
                        .iter()
                        .map(|signature| Signature::new(signature.parameters().clone(), None)),
                ),
                CallableTypeKind::ParamSpecValue,
            ));
            let param_spec_matches =
                ConstraintSet::constrain_typevar(db, self_bound_typevar, Type::Never, upper);
            let return_types_match = self.return_ty.when_some_and(|self_return_type| {
                other
                    .overloads
                    .iter()
                    .filter_map(|signature| signature.return_ty)
                    .when_any(db, |other_return_type| {
                        self_return_type.when_constraint_set_assignable_to(
                            db,
                            other_return_type,
                            inferable,
                        )
                    })
            });
            return param_spec_matches.and(db, || return_types_match);
        }

        other.overloads.iter().when_all(db, |other_signature| {
            self.when_constraint_set_assignable_to(db, other_signature, inferable)
        })
    }

    fn when_constraint_set_assignable_to(
        &self,
        db: &'db dyn Db,
        other: &Self,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.has_relation_to_impl(
            db,
            other,
            inferable,
            TypeRelation::ConstraintSetAssignability,
            &HasRelationToVisitor::default(),
            &IsDisjointVisitor::default(),
        )
    }

    /// Implementation of subtyping and assignability for signature.
    fn has_relation_to_impl(
        &self,
        db: &'db dyn Db,
        other: &Signature<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        // If either signature is generic, their typevars should also be considered inferable when
        // checking whether one signature is a subtype/etc of the other, since we only need to find
        // one specialization that causes the check to succeed.
        //
        // TODO: We should alpha-rename these typevars, too, to correctly handle when a generic
        // callable refers to typevars from within the context that defines them. This primarily
        // comes up when referring to a generic function recursively from within its body:
        //
        //     def identity[T](t: T) -> T:
        //         # Here, TypeOf[identity2] is a generic callable that should consider T to be
        //         # inferable, even though other uses of T in the function body are non-inferable.
        //         return t
        let self_inferable = self.inferable_typevars(db);
        let other_inferable = other.inferable_typevars(db);
        let inferable = inferable.merge(&self_inferable);
        let inferable = inferable.merge(&other_inferable);

        // `inner` will create a constraint set that references these newly inferable typevars.
        let when = self.has_relation_to_inner(
            db,
            other,
            inferable,
            relation,
            relation_visitor,
            disjointness_visitor,
        );

        // But the caller does not need to consider those extra typevars. Whatever constraint set
        // we produce, we reduce it back down to the inferable set that the caller asked about.
        // If we introduced new inferable typevars, those will be existentially quantified away
        // before returning.
        when.reduce_inferable(db, self_inferable.iter().chain(other_inferable.iter()))
    }

    fn has_relation_to_inner(
        &self,
        db: &'db dyn Db,
        other: &Signature<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        /// A helper struct to zip two slices of parameters together that provides control over the
        /// two iterators individually. It also keeps track of the current parameter in each
        /// iterator.
        struct ParametersZip<'a, 'db> {
            current_self: Option<&'a Parameter<'db>>,
            current_other: Option<&'a Parameter<'db>>,
            iter_self: Iter<'a, Parameter<'db>>,
            iter_other: Iter<'a, Parameter<'db>>,
        }

        impl<'a, 'db> ParametersZip<'a, 'db> {
            /// Move to the next parameter in both the `self` and `other` parameter iterators,
            /// [`None`] if both iterators are exhausted.
            fn next(&mut self) -> Option<EitherOrBoth<&'a Parameter<'db>, &'a Parameter<'db>>> {
                match (self.next_self(), self.next_other()) {
                    (Some(self_param), Some(other_param)) => {
                        Some(EitherOrBoth::Both(self_param, other_param))
                    }
                    (Some(self_param), None) => Some(EitherOrBoth::Left(self_param)),
                    (None, Some(other_param)) => Some(EitherOrBoth::Right(other_param)),
                    (None, None) => None,
                }
            }

            /// Move to the next parameter in the `self` parameter iterator, [`None`] if the
            /// iterator is exhausted.
            fn next_self(&mut self) -> Option<&'a Parameter<'db>> {
                self.current_self = self.iter_self.next();
                self.current_self
            }

            /// Move to the next parameter in the `other` parameter iterator, [`None`] if the
            /// iterator is exhausted.
            fn next_other(&mut self) -> Option<&'a Parameter<'db>> {
                self.current_other = self.iter_other.next();
                self.current_other
            }

            /// Peek at the next parameter in the `other` parameter iterator without consuming it.
            fn peek_other(&mut self) -> Option<&'a Parameter<'db>> {
                self.iter_other.clone().next()
            }

            /// Consumes the `ParametersZip` and returns a two-element tuple containing the
            /// remaining parameters in the `self` and `other` iterators respectively.
            ///
            /// The returned iterators starts with the current parameter, if any, followed by the
            /// remaining parameters in the respective iterators.
            fn into_remaining(
                self,
            ) -> (
                impl Iterator<Item = &'a Parameter<'db>>,
                impl Iterator<Item = &'a Parameter<'db>>,
            ) {
                (
                    self.current_self.into_iter().chain(self.iter_self),
                    self.current_other.into_iter().chain(self.iter_other),
                )
            }
        }

        let mut result = ConstraintSet::from(true);
        let mut check_types = |type1: Option<Type<'db>>, type2: Option<Type<'db>>| {
            let type1 = type1.unwrap_or(Type::unknown());
            let type2 = type2.unwrap_or(Type::unknown());

            match (type1, type2) {
                // This is a special case where the _same_ components of two different `ParamSpec`
                // type variables are assignable to each other when they're both in an inferable
                // position.
                //
                // `ParamSpec` type variables can only occur in parameter lists so this special case
                // is present here instead of in `Type::has_relation_to_impl`.
                (Type::TypeVar(typevar1), Type::TypeVar(typevar2))
                    if typevar1.paramspec_attr(db).is_some()
                        && typevar1.paramspec_attr(db) == typevar2.paramspec_attr(db)
                        && typevar1
                            .without_paramspec_attr(db)
                            .is_inferable(db, inferable)
                        && typevar2
                            .without_paramspec_attr(db)
                            .is_inferable(db, inferable) =>
                {
                    return true;
                }
                _ => {}
            }

            !result
                .intersect(
                    db,
                    type1.has_relation_to_impl(
                        db,
                        type2,
                        inferable,
                        relation,
                        relation_visitor,
                        disjointness_visitor,
                    ),
                )
                .is_never_satisfied(db)
        };

        // Return types are covariant.
        if !check_types(self.return_ty, other.return_ty) {
            return result;
        }

        // A gradual parameter list is a supertype of the "bottom" parameter list (*args: object,
        // **kwargs: object).
        if other.parameters.is_gradual()
            && self
                .parameters
                .variadic()
                .is_some_and(|(_, param)| param.annotated_type().is_some_and(|ty| ty.is_object()))
            && self
                .parameters
                .keyword_variadic()
                .is_some_and(|(_, param)| param.annotated_type().is_some_and(|ty| ty.is_object()))
        {
            return ConstraintSet::from(true);
        }

        // The top signature is supertype of (and assignable from) all other signatures. It is a
        // subtype of no signature except itself, and assignable only to the gradual signature.
        if other.parameters.is_top() {
            return ConstraintSet::from(true);
        } else if self.parameters.is_top() && !other.parameters.is_gradual() {
            return ConstraintSet::from(false);
        }

        // If either of the parameter lists is gradual (`...`), then it is assignable to and from
        // any other parameter list, but not a subtype or supertype of any other parameter list.
        if self.parameters.is_gradual() || other.parameters.is_gradual() {
            result.intersect(
                db,
                ConstraintSet::from(
                    relation.is_assignability() || relation.is_constraint_set_assignability(),
                ),
            );
            return result;
        }

        if relation.is_constraint_set_assignability() {
            let self_is_paramspec = self.parameters.as_paramspec();
            let other_is_paramspec = other.parameters.as_paramspec();

            // If either signature is a ParamSpec, the constraint set should bind the ParamSpec to
            // the other signature.
            match (self_is_paramspec, other_is_paramspec) {
                (Some(self_bound_typevar), Some(other_bound_typevar)) => {
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self_bound_typevar,
                        Type::TypeVar(other_bound_typevar),
                        Type::TypeVar(other_bound_typevar),
                    );
                    result.intersect(db, param_spec_matches);
                    return result;
                }

                (Some(self_bound_typevar), None) => {
                    let upper = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new(other.parameters.clone(), None)),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self_bound_typevar,
                        Type::Never,
                        upper,
                    );
                    result.intersect(db, param_spec_matches);
                    return result;
                }

                (None, Some(other_bound_typevar)) => {
                    let lower = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new(self.parameters.clone(), None)),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        other_bound_typevar,
                        lower,
                        Type::object(),
                    );
                    result.intersect(db, param_spec_matches);
                    return result;
                }

                (None, None) => {}
            }
        }

        let mut parameters = ParametersZip {
            current_self: None,
            current_other: None,
            iter_self: self.parameters.iter(),
            iter_other: other.parameters.iter(),
        };

        // Collect all the standard parameters that have only been matched against a variadic
        // parameter which means that the keyword variant is still unmatched.
        let mut other_keywords = Vec::new();

        loop {
            let Some(next_parameter) = parameters.next() else {
                // All parameters have been checked or both the parameter lists were empty. In
                // either case, `self` is a subtype of `other`.
                return result;
            };

            match next_parameter {
                EitherOrBoth::Left(self_parameter) => match self_parameter.kind() {
                    ParameterKind::KeywordOnly { .. } | ParameterKind::KeywordVariadic { .. }
                        if !other_keywords.is_empty() =>
                    {
                        // If there are any unmatched keyword parameters in `other`, they need to
                        // be checked against the keyword-only / keyword-variadic parameters that
                        // will be done after this loop.
                        break;
                    }
                    ParameterKind::PositionalOnly { default_type, .. }
                    | ParameterKind::PositionalOrKeyword { default_type, .. }
                    | ParameterKind::KeywordOnly { default_type, .. } => {
                        // For `self <: other` to be valid, if there are no more parameters in
                        // `other`, then the non-variadic parameters in `self` must have a default
                        // value.
                        if default_type.is_none() {
                            return ConstraintSet::from(false);
                        }
                    }
                    ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => {
                        // Variadic parameters don't have any restrictions in this context, so
                        // we'll just continue to the next parameter set.
                    }
                },

                EitherOrBoth::Right(_) => {
                    // If there are more parameters in `other` than in `self`, then `self` is not a
                    // subtype of `other`.
                    return ConstraintSet::from(false);
                }

                EitherOrBoth::Both(self_parameter, other_parameter) => {
                    match (self_parameter.kind(), other_parameter.kind()) {
                        (
                            ParameterKind::PositionalOnly {
                                default_type: self_default,
                                ..
                            }
                            | ParameterKind::PositionalOrKeyword {
                                default_type: self_default,
                                ..
                            },
                            ParameterKind::PositionalOnly {
                                default_type: other_default,
                                ..
                            },
                        ) => {
                            if self_default.is_none() && other_default.is_some() {
                                return ConstraintSet::from(false);
                            }
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return result;
                            }
                        }

                        (
                            ParameterKind::PositionalOrKeyword {
                                name: self_name,
                                default_type: self_default,
                            },
                            ParameterKind::PositionalOrKeyword {
                                name: other_name,
                                default_type: other_default,
                            },
                        ) => {
                            if self_name != other_name {
                                return ConstraintSet::from(false);
                            }
                            // The following checks are the same as positional-only parameters.
                            if self_default.is_none() && other_default.is_some() {
                                return ConstraintSet::from(false);
                            }
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return result;
                            }
                        }

                        (
                            ParameterKind::Variadic { .. },
                            ParameterKind::PositionalOnly { .. }
                            | ParameterKind::PositionalOrKeyword { .. },
                        ) => {
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return result;
                            }

                            if matches!(
                                other_parameter.kind(),
                                ParameterKind::PositionalOrKeyword { .. }
                            ) {
                                other_keywords.push(other_parameter);
                            }

                            // We've reached a variadic parameter in `self` which means there can
                            // be no more positional parameters after this in a valid AST. But, the
                            // current parameter in `other` is a positional-only which means there
                            // can be more positional parameters after this which could be either
                            // more positional-only parameters, standard parameters or a variadic
                            // parameter.
                            //
                            // So, any remaining positional parameters in `other` would need to be
                            // checked against the variadic parameter in `self`. This loop does
                            // that by only moving the `other` iterator forward.
                            loop {
                                let Some(other_parameter) = parameters.peek_other() else {
                                    break;
                                };
                                match other_parameter.kind() {
                                    ParameterKind::PositionalOrKeyword { .. } => {
                                        other_keywords.push(other_parameter);
                                    }
                                    ParameterKind::PositionalOnly { .. }
                                    | ParameterKind::Variadic { .. } => {}
                                    _ => {
                                        // Any other parameter kind cannot be checked against a
                                        // variadic parameter and is deferred to the next iteration.
                                        break;
                                    }
                                }
                                if !check_types(
                                    other_parameter.annotated_type(),
                                    self_parameter.annotated_type(),
                                ) {
                                    return result;
                                }
                                parameters.next_other();
                            }
                        }

                        (ParameterKind::Variadic { .. }, ParameterKind::Variadic { .. }) => {
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return result;
                            }
                        }

                        (
                            _,
                            ParameterKind::KeywordOnly { .. }
                            | ParameterKind::KeywordVariadic { .. },
                        ) => {
                            // Keyword parameters are not considered in this loop as the order of
                            // parameters is not important for them and so they are checked by
                            // doing name-based lookups.
                            break;
                        }

                        _ => return ConstraintSet::from(false),
                    }
                }
            }
        }

        // At this point, the remaining parameters in `other` are keyword-only or keyword variadic.
        // But, `self` could contain any unmatched positional parameters.
        let (self_parameters, other_parameters) = parameters.into_remaining();

        // Collect all the keyword-only parameters and the unmatched standard parameters.
        let mut self_keywords = FxHashMap::default();

        // Type of the variadic keyword parameter in `self`.
        //
        // This is a nested option where the outer option represents the presence of a keyword
        // variadic parameter in `self` and the inner option represents the annotated type of the
        // keyword variadic parameter.
        let mut self_keyword_variadic: Option<Option<Type<'db>>> = None;

        for self_parameter in self_parameters {
            match self_parameter.kind() {
                ParameterKind::KeywordOnly { name, .. }
                | ParameterKind::PositionalOrKeyword { name, .. } => {
                    self_keywords.insert(name.as_str(), self_parameter);
                }
                ParameterKind::KeywordVariadic { .. } => {
                    self_keyword_variadic = Some(self_parameter.annotated_type());
                }
                ParameterKind::PositionalOnly { .. } => {
                    // These are the unmatched positional-only parameters in `self` from the
                    // previous loop. They cannot be matched against any parameter in `other` which
                    // only contains keyword-only and keyword-variadic parameters so the subtype
                    // relation is invalid.
                    return ConstraintSet::from(false);
                }
                ParameterKind::Variadic { .. } => {}
            }
        }

        for other_parameter in other_keywords.into_iter().chain(other_parameters) {
            match other_parameter.kind() {
                ParameterKind::KeywordOnly {
                    name: other_name,
                    default_type: other_default,
                }
                | ParameterKind::PositionalOrKeyword {
                    name: other_name,
                    default_type: other_default,
                } => {
                    if let Some(self_parameter) = self_keywords.remove(other_name.as_str()) {
                        match self_parameter.kind() {
                            ParameterKind::PositionalOrKeyword {
                                default_type: self_default,
                                ..
                            }
                            | ParameterKind::KeywordOnly {
                                default_type: self_default,
                                ..
                            } => {
                                if self_default.is_none() && other_default.is_some() {
                                    return ConstraintSet::from(false);
                                }
                                if !check_types(
                                    other_parameter.annotated_type(),
                                    self_parameter.annotated_type(),
                                ) {
                                    return result;
                                }
                            }
                            _ => unreachable!(
                                "`self_keywords` should only contain keyword-only or standard parameters"
                            ),
                        }
                    } else if let Some(self_keyword_variadic_type) = self_keyword_variadic {
                        if !check_types(
                            other_parameter.annotated_type(),
                            self_keyword_variadic_type,
                        ) {
                            return result;
                        }
                    } else {
                        return ConstraintSet::from(false);
                    }
                }
                ParameterKind::KeywordVariadic { .. } => {
                    let Some(self_keyword_variadic_type) = self_keyword_variadic else {
                        // For a `self <: other` relationship, if `other` has a keyword variadic
                        // parameter, `self` must also have a keyword variadic parameter.
                        return ConstraintSet::from(false);
                    };
                    if !check_types(other_parameter.annotated_type(), self_keyword_variadic_type) {
                        return result;
                    }
                }
                _ => {
                    // This can only occur in case of a syntax error.
                    return ConstraintSet::from(false);
                }
            }
        }

        // If there are still unmatched keyword parameters from `self`, then they should be
        // optional otherwise the subtype relation is invalid.
        for (_, self_parameter) in self_keywords {
            if self_parameter.default_type().is_none() {
                return ConstraintSet::from(false);
            }
        }

        result
    }

    /// Create a new signature with the given definition.
    pub(crate) fn with_definition(self, definition: Option<Definition<'db>>) -> Self {
        Self { definition, ..self }
    }
}

impl<'db> VarianceInferable<'db> for &Signature<'db> {
    fn variance_of(self, db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> TypeVarVariance {
        tracing::trace!(
            "Checking variance of `{tvar}` in `{self:?}`",
            tvar = typevar.typevar(db).name(db)
        );
        itertools::chain(
            self.parameters
                .iter()
                .filter_map(|parameter| match parameter.form {
                    ParameterForm::Type => None,
                    ParameterForm::Value => parameter.annotated_type().map(|ty| {
                        ty.with_polarity(TypeVarVariance::Contravariant)
                            .variance_of(db, typevar)
                    }),
                }),
            self.return_ty.map(|ty| ty.variance_of(db, typevar)),
        )
        .collect()
    }
}

// TODO: the spec also allows signatures like `Concatenate[int, ...]` or `Concatenate[int, P]`,
// which have some number of required positional-only parameters followed by a gradual form or a
// `ParamSpec`. Our representation will need some adjustments to represent that.

/// The kind of parameter list represented.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum ParametersKind<'db> {
    /// A standard parameter list.
    #[default]
    Standard,

    /// Represents a gradual parameter list using `...` as the only parameter.
    ///
    /// Per [the typing specification], any signature with a variadic and a keyword-variadic
    /// argument, both annotated (explicitly or implicitly) as `Any` or `Unknown`, is considered
    /// equivalent to `...`.
    ///
    /// The display implementation utilizes this flag to use `...` instead of displaying the
    /// individual variadic and keyword-variadic parameters.
    ///
    /// Note: This flag can also result from invalid forms of `Callable` annotations.
    ///
    /// [the typing specification]: https://typing.python.org/en/latest/spec/callables.html#meaning-of-in-callable
    Gradual,

    /// Represents the "top" parameters: top materialization of Gradual parameters, or infinite
    /// union of all possible parameter signatures.
    Top,

    /// Represents a parameter list containing a `ParamSpec` as the only parameter.
    ///
    /// Note that this is distinct from a parameter list _containing_ a `ParamSpec` which is
    /// considered a standard parameter list that just contains a `ParamSpec`.
    // TODO: Maybe we should use `find_paramspec_from_args_kwargs` instead of storing the typevar
    // here?
    ParamSpec(BoundTypeVarInstance<'db>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct Parameters<'db> {
    // TODO: use SmallVec here once invariance bug is fixed
    value: Vec<Parameter<'db>>,
    kind: ParametersKind<'db>,
}

impl<'db> Parameters<'db> {
    /// Create a new parameter list from an iterator of parameters.
    ///
    /// The kind of the parameter list is determined based on the provided parameters.
    /// Specifically, if the parameters is made up of `*args` and `**kwargs` only, it checks
    /// their annotated types to determine if they represent a gradual form or a `ParamSpec`.
    pub(crate) fn new(
        db: &'db dyn Db,
        parameters: impl IntoIterator<Item = Parameter<'db>>,
    ) -> Self {
        fn new_impl<'db>(db: &'db dyn Db, value: Vec<Parameter<'db>>) -> Parameters<'db> {
            let mut kind = ParametersKind::Standard;
            if let [p1, p2] = value.as_slice()
                && p1.is_variadic()
                && p2.is_keyword_variadic()
            {
                match (p1.annotated_type(), p2.annotated_type()) {
                    (None | Some(Type::Dynamic(_)), None | Some(Type::Dynamic(_))) => {
                        kind = ParametersKind::Gradual;
                    }
                    (Some(Type::TypeVar(args_typevar)), Some(Type::TypeVar(kwargs_typevar))) => {
                        if let (Some(ParamSpecAttrKind::Args), Some(ParamSpecAttrKind::Kwargs)) = (
                            args_typevar.paramspec_attr(db),
                            kwargs_typevar.paramspec_attr(db),
                        ) {
                            let typevar = args_typevar.without_paramspec_attr(db);
                            if typevar
                                .is_same_typevar_as(db, kwargs_typevar.without_paramspec_attr(db))
                            {
                                kind = ParametersKind::ParamSpec(typevar);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Parameters { value, kind }
        }

        let value: Vec<Parameter<'db>> = parameters.into_iter().collect();
        new_impl(db, value)
    }

    /// Create an empty parameter list.
    pub(crate) fn empty() -> Self {
        Self {
            value: Vec::new(),
            kind: ParametersKind::Standard,
        }
    }

    pub(crate) fn as_slice(&self) -> &[Parameter<'db>] {
        self.value.as_slice()
    }

    pub(crate) const fn kind(&self) -> ParametersKind<'db> {
        self.kind
    }

    pub(crate) const fn is_gradual(&self) -> bool {
        matches!(self.kind, ParametersKind::Gradual)
    }

    pub(crate) const fn is_top(&self) -> bool {
        matches!(self.kind, ParametersKind::Top)
    }

    pub(crate) const fn as_paramspec(&self) -> Option<BoundTypeVarInstance<'db>> {
        match self.kind {
            ParametersKind::ParamSpec(bound_typevar) => Some(bound_typevar),
            _ => None,
        }
    }

    /// Return todo parameters: (*args: Todo, **kwargs: Todo)
    pub(crate) fn todo() -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(todo_type!("todo signature *args")),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(todo_type!("todo signature **kwargs")),
            ],
            kind: ParametersKind::Gradual,
        }
    }

    /// Return parameters that represents a gradual form using `...` as the only parameter.
    ///
    /// Internally, this is represented as `(*Any, **Any)` that accepts parameters of type [`Any`].
    ///
    /// [`Any`]: crate::types::DynamicType::Any
    pub(crate) fn gradual_form() -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(Type::Dynamic(DynamicType::Any)),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(Type::Dynamic(DynamicType::Any)),
            ],
            kind: ParametersKind::Gradual,
        }
    }

    pub(crate) fn paramspec(db: &'db dyn Db, typevar: BoundTypeVarInstance<'db>) -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::TypeVar(
                    typevar.with_paramspec_attr(db, ParamSpecAttrKind::Args),
                )),
                Parameter::keyword_variadic(Name::new_static("kwargs")).with_annotated_type(
                    Type::TypeVar(typevar.with_paramspec_attr(db, ParamSpecAttrKind::Kwargs)),
                ),
            ],
            kind: ParametersKind::ParamSpec(typevar),
        }
    }

    /// Return parameters that represents an unknown list of parameters.
    ///
    /// Internally, this is represented as `(*Unknown, **Unknown)` that accepts parameters of type
    /// [`Unknown`].
    ///
    /// [`Unknown`]: crate::types::DynamicType::Unknown
    pub(crate) fn unknown() -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(Type::Dynamic(DynamicType::Unknown)),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(Type::Dynamic(DynamicType::Unknown)),
            ],
            kind: ParametersKind::Gradual,
        }
    }

    /// Return parameters that represents `(*args: object, **kwargs: object)`, the bottom signature
    /// (accepts any call, so subtype of all other signatures.)
    pub(crate) fn bottom() -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::object()),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(Type::object()),
            ],
            kind: ParametersKind::Standard,
        }
    }

    /// Return the "top" parameters (infinite union of all possible parameters), which cannot
    /// accept any call, since there is no possible call that satisfies all possible parameter
    /// signatures. This is not `(*Never, **Never)`, which is equivalent to no parameters at all
    /// and still accepts the empty call `()`; it has to be represented instead as a special
    /// `ParametersKind`.
    pub(crate) fn top() -> Self {
        Self {
            // We always emit `called-top-callable` for any call to the top callable (based on the
            // `kind` below), so we otherwise give it the most permissive signature`(*object,
            // **object)`, so that we avoid emitting any other errors about arity mismatches.
            value: vec![
                Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::object()),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(Type::object()),
            ],
            kind: ParametersKind::Top,
        }
    }

    /// Returns the bound `ParamSpec` type variable if the parameters contain a `ParamSpec`.
    pub(crate) fn find_paramspec_from_args_kwargs<'a>(
        &'a self,
        db: &'db dyn Db,
    ) -> Option<(&'a [Parameter<'db>], BoundTypeVarInstance<'db>)> {
        let [prefix @ .., maybe_args, maybe_kwargs] = self.value.as_slice() else {
            return None;
        };

        if !maybe_args.is_variadic() || !maybe_kwargs.is_keyword_variadic() {
            return None;
        }

        let (Type::TypeVar(args_typevar), Type::TypeVar(kwargs_typevar)) =
            (maybe_args.annotated_type()?, maybe_kwargs.annotated_type()?)
        else {
            return None;
        };

        if matches!(
            (
                args_typevar.paramspec_attr(db),
                kwargs_typevar.paramspec_attr(db)
            ),
            (
                Some(ParamSpecAttrKind::Args),
                Some(ParamSpecAttrKind::Kwargs)
            )
        ) {
            let typevar = args_typevar.without_paramspec_attr(db);
            if typevar.is_same_typevar_as(db, kwargs_typevar.without_paramspec_attr(db)) {
                return Some((prefix, typevar));
            }
        }

        None
    }

    fn from_parameters(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameters: &ast::Parameters,
        has_implicitly_positional_first_parameter: bool,
    ) -> Self {
        let ast::Parameters {
            posonlyargs,
            args,
            vararg,
            kwonlyargs,
            kwarg,
            range: _,
            node_index: _,
        } = parameters;

        let default_type = |param: &ast::ParameterWithDefault| {
            param.default().map(|default| {
                definition_expression_type(db, definition, default).replace_parameter_defaults(db)
            })
        };

        let pos_only_param = |param: &ast::ParameterWithDefault| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &param.parameter,
                ParameterKind::PositionalOnly {
                    name: Some(param.parameter.name.id.clone()),
                    default_type: default_type(param),
                },
            )
        };

        let mut positional_only: Vec<Parameter> = posonlyargs.iter().map(pos_only_param).collect();

        let mut pos_or_keyword_iter = args.iter();

        // If there are no PEP-570 positional-only parameters, check for the legacy PEP-484 convention
        // for denoting positional-only parameters (parameters that start with `__` and do not end with `__`)
        if positional_only.is_empty() {
            let pos_or_keyword_iter = pos_or_keyword_iter.by_ref();

            if has_implicitly_positional_first_parameter {
                positional_only.extend(pos_or_keyword_iter.next().map(pos_only_param));
            }

            positional_only.extend(
                pos_or_keyword_iter
                    .peeking_take_while(|param| param.uses_pep_484_positional_only_convention())
                    .map(pos_only_param),
            );
        }

        let positional_or_keyword = pos_or_keyword_iter.map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::PositionalOrKeyword {
                    name: arg.parameter.name.id.clone(),
                    default_type: default_type(arg),
                },
            )
        });

        let variadic = vararg.as_ref().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                arg,
                ParameterKind::Variadic {
                    name: arg.name.id.clone(),
                },
            )
        });

        let keyword_only = kwonlyargs.iter().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::KeywordOnly {
                    name: arg.parameter.name.id.clone(),
                    default_type: default_type(arg),
                },
            )
        });

        let keywords = kwarg.as_ref().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                arg,
                ParameterKind::KeywordVariadic {
                    name: arg.name.id.clone(),
                },
            )
        });

        Self::new(
            db,
            positional_only
                .into_iter()
                .chain(positional_or_keyword)
                .chain(variadic)
                .chain(keyword_only)
                .chain(keywords),
        )
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        if let TypeMapping::Materialize(materialization_kind) = type_mapping
            && self.kind == ParametersKind::Gradual
        {
            match materialization_kind {
                MaterializationKind::Bottom => {
                    // The bottom materialization of the `...` parameters is `(*object, **object)`,
                    // which accepts any call and is thus a subtype of all other parameters.
                    return Parameters::bottom();
                }
                MaterializationKind::Top => {
                    return Parameters::top();
                }
            }
        }

        // Parameters are in contravariant position, so we need to flip the type mapping.
        let type_mapping = type_mapping.flip();

        Self {
            value: self
                .value
                .iter()
                .map(|param| param.apply_type_mapping_impl(db, &type_mapping, tcx, visitor))
                .collect(),
            kind: self.kind,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.value.len()
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Parameter<'db>> {
        self.value.iter()
    }

    /// Iterate initial positional parameters, not including variadic parameter, if any.
    ///
    /// For a valid signature, this will be all positional parameters. In an invalid signature,
    /// there could be non-initial positional parameters; effectively, we just won't consider those
    /// to be positional, which is fine.
    pub(crate) fn positional(&self) -> impl Iterator<Item = &Parameter<'db>> {
        self.iter().take_while(|param| param.is_positional())
    }

    /// Return parameter at given index, or `None` if index is out-of-range.
    pub(crate) fn get(&self, index: usize) -> Option<&Parameter<'db>> {
        self.value.get(index)
    }

    /// Return positional parameter at given index, or `None` if `index` is out of range.
    ///
    /// Does not return variadic parameter.
    pub(crate) fn get_positional(&self, index: usize) -> Option<&Parameter<'db>> {
        self.get(index)
            .and_then(|parameter| parameter.is_positional().then_some(parameter))
    }

    /// Return a positional-only parameter (with index) with the given name.
    pub(crate) fn positional_only_by_name(&self, name: &str) -> Option<(usize, &Parameter<'db>)> {
        self.iter().enumerate().find(|(_, parameter)| {
            parameter.is_positional_only()
                && parameter
                    .name()
                    .map(|p_name| p_name == name)
                    .unwrap_or(false)
        })
    }

    /// Return the variadic parameter (`*args`), if any, and its index, or `None`.
    pub(crate) fn variadic(&self) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .find(|(_, parameter)| parameter.is_variadic())
    }

    /// Return parameter (with index) for given name, or `None` if no such parameter.
    ///
    /// Does not return keywords (`**kwargs`) parameter.
    ///
    /// In an invalid signature, there could be multiple parameters with the same name; we will
    /// just return the first that matches.
    pub(crate) fn keyword_by_name(&self, name: &str) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .find(|(_, parameter)| parameter.callable_by_name(name))
    }

    /// Return the keywords parameter (`**kwargs`), if any, and its index, or `None`.
    pub(crate) fn keyword_variadic(&self) -> Option<(usize, &Parameter<'db>)> {
        self.iter()
            .enumerate()
            .rfind(|(_, parameter)| parameter.is_keyword_variadic())
    }
}

impl<'db, 'a> IntoIterator for &'a Parameters<'db> {
    type Item = &'a Parameter<'db>;
    type IntoIter = std::slice::Iter<'a, Parameter<'db>>;

    fn into_iter(self) -> Self::IntoIter {
        self.value.iter()
    }
}

impl<'db> std::ops::Index<usize> for Parameters<'db> {
    type Output = Parameter<'db>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.value[index]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct Parameter<'db> {
    /// Annotated type of the parameter.
    annotated_type: Option<Type<'db>>,

    /// Does the type of this parameter come from an explicit annotation, or was it inferred from
    /// the context, like `Self` for the `self` parameter of instance methods.
    pub(crate) inferred_annotation: bool,

    kind: ParameterKind<'db>,
    pub(crate) form: ParameterForm,
}

impl<'db> Parameter<'db> {
    pub(crate) fn positional_only(name: Option<Name>) -> Self {
        Self {
            annotated_type: None,
            inferred_annotation: false,
            kind: ParameterKind::PositionalOnly {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn positional_or_keyword(name: Name) -> Self {
        Self {
            annotated_type: None,
            inferred_annotation: false,
            kind: ParameterKind::PositionalOrKeyword {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn variadic(name: Name) -> Self {
        Self {
            annotated_type: None,
            inferred_annotation: false,
            kind: ParameterKind::Variadic { name },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn keyword_only(name: Name) -> Self {
        Self {
            annotated_type: None,
            inferred_annotation: false,
            kind: ParameterKind::KeywordOnly {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn keyword_variadic(name: Name) -> Self {
        Self {
            annotated_type: None,
            inferred_annotation: false,
            kind: ParameterKind::KeywordVariadic { name },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn with_annotated_type(mut self, annotated_type: Type<'db>) -> Self {
        self.annotated_type = Some(annotated_type);
        self
    }

    pub(crate) fn with_default_type(mut self, default: Type<'db>) -> Self {
        match &mut self.kind {
            ParameterKind::PositionalOnly { default_type, .. }
            | ParameterKind::PositionalOrKeyword { default_type, .. }
            | ParameterKind::KeywordOnly { default_type, .. } => *default_type = Some(default),
            ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => {
                panic!("cannot set default value for variadic parameter")
            }
        }
        self
    }

    pub(crate) fn type_form(mut self) -> Self {
        self.form = ParameterForm::Type;
        self
    }

    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        Self {
            annotated_type: self
                .annotated_type
                .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)),
            kind: self
                .kind
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            inferred_annotation: self.inferred_annotation,
            form: self.form,
        }
    }

    /// Strip information from the parameter so that two equivalent parameters compare equal.
    /// Normalize nested unions and intersections in the annotated type, if any.
    ///
    /// See [`Type::normalized`] for more details.
    pub(crate) fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> Self {
        let Parameter {
            annotated_type,
            inferred_annotation,
            kind,
            form,
        } = self;

        // Ensure unions and intersections are ordered in the annotated type (if there is one).
        // Ensure that a parameter without an annotation is treated equivalently to a parameter
        // with a dynamic type as its annotation. (We must use `Any` here as all dynamic types
        // normalize to `Any`.)
        let annotated_type = annotated_type
            .map(|ty| ty.normalized_impl(db, visitor))
            .unwrap_or_else(Type::any);

        // Ensure that parameter names are stripped from positional-only, variadic and keyword-variadic parameters.
        // Ensure that we only record whether a parameter *has* a default
        // (strip the precise *type* of the default from the parameter, replacing it with `Never`).
        let kind = match kind {
            ParameterKind::PositionalOnly {
                name: _,
                default_type,
            } => ParameterKind::PositionalOnly {
                name: None,
                default_type: default_type.map(|_| Type::Never),
            },
            ParameterKind::PositionalOrKeyword { name, default_type } => {
                ParameterKind::PositionalOrKeyword {
                    name: name.clone(),
                    default_type: default_type.map(|_| Type::Never),
                }
            }
            ParameterKind::KeywordOnly { name, default_type } => ParameterKind::KeywordOnly {
                name: name.clone(),
                default_type: default_type.map(|_| Type::Never),
            },
            ParameterKind::Variadic { name: _ } => ParameterKind::Variadic {
                name: Name::new_static("args"),
            },
            ParameterKind::KeywordVariadic { name: _ } => ParameterKind::KeywordVariadic {
                name: Name::new_static("kwargs"),
            },
        };

        Self {
            annotated_type: Some(annotated_type),
            inferred_annotation: *inferred_annotation,
            kind,
            form: *form,
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let Parameter {
            annotated_type,
            inferred_annotation,
            kind,
            form,
        } = self;

        let annotated_type = match annotated_type {
            Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, div, true)?),
            Some(ty) => Some(
                ty.recursive_type_normalized_impl(db, div, true)
                    .unwrap_or(div),
            ),
            None => None,
        };

        let kind = match kind {
            ParameterKind::PositionalOnly { name, default_type } => ParameterKind::PositionalOnly {
                name: name.clone(),
                default_type: match default_type {
                    Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, div, true)?),
                    Some(ty) => Some(
                        ty.recursive_type_normalized_impl(db, div, true)
                            .unwrap_or(div),
                    ),
                    None => None,
                },
            },
            ParameterKind::PositionalOrKeyword { name, default_type } => {
                ParameterKind::PositionalOrKeyword {
                    name: name.clone(),
                    default_type: match default_type {
                        Some(ty) if nested => {
                            Some(ty.recursive_type_normalized_impl(db, div, true)?)
                        }
                        Some(ty) => Some(
                            ty.recursive_type_normalized_impl(db, div, true)
                                .unwrap_or(div),
                        ),
                        None => None,
                    },
                }
            }
            ParameterKind::KeywordOnly { name, default_type } => ParameterKind::KeywordOnly {
                name: name.clone(),
                default_type: match default_type {
                    Some(ty) if nested => Some(ty.recursive_type_normalized_impl(db, div, true)?),
                    Some(ty) => Some(
                        ty.recursive_type_normalized_impl(db, div, true)
                            .unwrap_or(div),
                    ),
                    None => None,
                },
            },
            ParameterKind::Variadic { name } => ParameterKind::Variadic { name: name.clone() },
            ParameterKind::KeywordVariadic { name } => {
                ParameterKind::KeywordVariadic { name: name.clone() }
            }
        };

        Some(Self {
            annotated_type,
            inferred_annotation: *inferred_annotation,
            kind,
            form: *form,
        })
    }

    fn from_node_and_kind(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameter: &ast::Parameter,
        kind: ParameterKind<'db>,
    ) -> Self {
        Self {
            annotated_type: parameter
                .annotation()
                .map(|annotation| function_signature_expression_type(db, definition, annotation)),
            kind,
            form: ParameterForm::Value,
            inferred_annotation: false,
        }
    }

    /// Returns `true` if this is a keyword-only parameter.
    pub(crate) fn is_keyword_only(&self) -> bool {
        matches!(self.kind, ParameterKind::KeywordOnly { .. })
    }

    /// Returns `true` if this is a positional-only parameter.
    pub(crate) fn is_positional_only(&self) -> bool {
        matches!(self.kind, ParameterKind::PositionalOnly { .. })
    }

    /// Returns `true` if this is a variadic parameter.
    pub(crate) fn is_variadic(&self) -> bool {
        matches!(self.kind, ParameterKind::Variadic { .. })
    }

    /// Returns `true` if this is a keyword-variadic parameter.
    pub(crate) fn is_keyword_variadic(&self) -> bool {
        matches!(self.kind, ParameterKind::KeywordVariadic { .. })
    }

    /// Returns `true` if this is either a positional-only or standard (positional or keyword)
    /// parameter.
    pub(crate) fn is_positional(&self) -> bool {
        matches!(
            self.kind,
            ParameterKind::PositionalOnly { .. } | ParameterKind::PositionalOrKeyword { .. }
        )
    }

    pub(crate) fn callable_by_name(&self, name: &str) -> bool {
        match &self.kind {
            ParameterKind::PositionalOrKeyword {
                name: param_name, ..
            }
            | ParameterKind::KeywordOnly {
                name: param_name, ..
            } => param_name == name,
            _ => false,
        }
    }

    /// Annotated type of the parameter, if annotated.
    pub(crate) fn annotated_type(&self) -> Option<Type<'db>> {
        self.annotated_type
    }

    /// Kind of the parameter.
    pub(crate) fn kind(&self) -> &ParameterKind<'db> {
        &self.kind
    }

    /// Whether or not the type of this parameter should be displayed.
    pub(crate) fn should_annotation_be_displayed(&self) -> bool {
        !self.inferred_annotation
    }

    /// Name of the parameter (if it has one).
    pub(crate) fn name(&self) -> Option<&ast::name::Name> {
        match &self.kind {
            ParameterKind::PositionalOnly { name, .. } => name.as_ref(),
            ParameterKind::PositionalOrKeyword { name, .. } => Some(name),
            ParameterKind::Variadic { name } => Some(name),
            ParameterKind::KeywordOnly { name, .. } => Some(name),
            ParameterKind::KeywordVariadic { name } => Some(name),
        }
    }

    /// Display name of the parameter, if it has one.
    pub(crate) fn display_name(&self) -> Option<ast::name::Name> {
        self.name().map(|name| match self.kind {
            ParameterKind::Variadic { .. } => ast::name::Name::new(format!("*{name}")),
            ParameterKind::KeywordVariadic { .. } => ast::name::Name::new(format!("**{name}")),
            _ => name.clone(),
        })
    }

    /// Default-value type of the parameter, if any.
    pub(crate) fn default_type(&self) -> Option<Type<'db>> {
        match self.kind {
            ParameterKind::PositionalOnly { default_type, .. }
            | ParameterKind::PositionalOrKeyword { default_type, .. }
            | ParameterKind::KeywordOnly { default_type, .. } => default_type,
            ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub enum ParameterKind<'db> {
    /// Positional-only parameter, e.g. `def f(x, /): ...`
    PositionalOnly {
        /// Parameter name.
        ///
        /// It is possible for signatures to be defined in ways that leave positional-only parameters
        /// nameless (e.g. via `Callable` annotations).
        name: Option<Name>,
        default_type: Option<Type<'db>>,
    },

    /// Positional-or-keyword parameter, e.g. `def f(x): ...`
    PositionalOrKeyword {
        /// Parameter name.
        name: Name,
        default_type: Option<Type<'db>>,
    },

    /// Variadic parameter, e.g. `def f(*args): ...`
    Variadic {
        /// Parameter name.
        name: Name,
    },

    /// Keyword-only parameter, e.g. `def f(*, x): ...`
    KeywordOnly {
        /// Parameter name.
        name: Name,
        default_type: Option<Type<'db>>,
    },

    /// Variadic keywords parameter, e.g. `def f(**kwargs): ...`
    KeywordVariadic {
        /// Parameter name.
        name: Name,
    },
}

impl<'db> ParameterKind<'db> {
    fn apply_type_mapping_impl<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let apply_to_default_type = |default_type: &Option<Type<'db>>| {
            if type_mapping == &TypeMapping::ReplaceParameterDefaults && default_type.is_some() {
                Some(Type::unknown())
            } else {
                default_type
                    .as_ref()
                    .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor))
            }
        };

        match self {
            Self::PositionalOnly { default_type, name } => Self::PositionalOnly {
                default_type: apply_to_default_type(default_type),
                name: name.clone(),
            },
            Self::PositionalOrKeyword { default_type, name } => Self::PositionalOrKeyword {
                default_type: apply_to_default_type(default_type),
                name: name.clone(),
            },
            Self::KeywordOnly { default_type, name } => Self::KeywordOnly {
                default_type: apply_to_default_type(default_type),
                name: name.clone(),
            },
            Self::Variadic { .. } | Self::KeywordVariadic { .. } => self.clone(),
        }
    }
}

/// Whether a parameter is used as a value or a type form.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub(crate) enum ParameterForm {
    Value,
    Type,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::{TestDb, setup_db};
    use crate::place::global_symbol;
    use crate::types::{FunctionType, KnownClass};
    use ruff_db::system::DbWithWritableSystem as _;

    #[track_caller]
    fn get_function_f<'db>(db: &'db TestDb, file: &'static str) -> FunctionType<'db> {
        let module = ruff_db::files::system_path_to_file(db, file).unwrap();
        global_symbol(db, module, "f")
            .place
            .expect_type()
            .expect_function_literal()
    }

    #[track_caller]
    fn assert_params<'db>(signature: &Signature<'db>, expected: &[Parameter<'db>]) {
        assert_eq!(signature.parameters.value.as_slice(), expected);
    }

    #[test]
    fn empty() {
        let mut db = setup_db();
        db.write_dedented("/src/a.py", "def f(): ...").unwrap();
        let func = get_function_f(&db, "/src/a.py")
            .literal(&db)
            .last_definition(&db);

        let sig = func.signature(&db);

        assert!(sig.return_ty.is_none());
        assert_params(&sig, &[]);
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn full() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            from typing import Literal

            def f(a, b: int, c = 1, d: int = 2, /,
                  e = 3, f: Literal[4] = 4, *args: object,
                  g = 5, h: Literal[6] = 6, **kwargs: str) -> bytes: ...
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py")
            .literal(&db)
            .last_definition(&db);

        let sig = func.signature(&db);

        assert_eq!(sig.return_ty.unwrap().display(&db).to_string(), "bytes");
        assert_params(
            &sig,
            &[
                Parameter::positional_only(Some(Name::new_static("a"))),
                Parameter::positional_only(Some(Name::new_static("b")))
                    .with_annotated_type(KnownClass::Int.to_instance(&db)),
                Parameter::positional_only(Some(Name::new_static("c")))
                    .with_default_type(Type::IntLiteral(1)),
                Parameter::positional_only(Some(Name::new_static("d")))
                    .with_annotated_type(KnownClass::Int.to_instance(&db))
                    .with_default_type(Type::IntLiteral(2)),
                Parameter::positional_or_keyword(Name::new_static("e"))
                    .with_default_type(Type::IntLiteral(3)),
                Parameter::positional_or_keyword(Name::new_static("f"))
                    .with_annotated_type(Type::IntLiteral(4))
                    .with_default_type(Type::IntLiteral(4)),
                Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::object()),
                Parameter::keyword_only(Name::new_static("g"))
                    .with_default_type(Type::IntLiteral(5)),
                Parameter::keyword_only(Name::new_static("h"))
                    .with_annotated_type(Type::IntLiteral(6))
                    .with_default_type(Type::IntLiteral(6)),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(KnownClass::Str.to_instance(&db)),
            ],
        );
    }

    #[test]
    fn not_deferred() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            class A: ...
            class B: ...

            alias = A

            def f(a: alias): ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py")
            .literal(&db)
            .last_definition(&db);

        let sig = func.signature(&db);

        let [
            Parameter {
                annotated_type,
                kind: ParameterKind::PositionalOrKeyword { name, .. },
                ..
            },
        ] = &sig.parameters.value[..]
        else {
            panic!("expected one positional-or-keyword parameter");
        };
        assert_eq!(name, "a");
        // Parameter resolution not deferred; we should see A not B
        assert_eq!(annotated_type.unwrap().display(&db).to_string(), "A");
    }

    #[test]
    fn deferred_in_stub() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.pyi",
            "
            class A: ...
            class B: ...

            alias = A

            def f(a: alias): ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.pyi")
            .literal(&db)
            .last_definition(&db);

        let sig = func.signature(&db);

        let [
            Parameter {
                annotated_type,
                kind: ParameterKind::PositionalOrKeyword { name, .. },
                ..
            },
        ] = &sig.parameters.value[..]
        else {
            panic!("expected one positional-or-keyword parameter");
        };
        assert_eq!(name, "a");
        // Parameter resolution deferred:
        assert_eq!(annotated_type.unwrap().display(&db).to_string(), "A | B");
    }

    #[test]
    fn generic_not_deferred() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            class A: ...
            class B: ...

            alias = A

            def f[T](a: alias, b: T) -> T: ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py")
            .literal(&db)
            .last_definition(&db);

        let sig = func.signature(&db);

        let [
            Parameter {
                annotated_type: a_annotated_ty,
                kind: ParameterKind::PositionalOrKeyword { name: a_name, .. },
                ..
            },
            Parameter {
                annotated_type: b_annotated_ty,
                kind: ParameterKind::PositionalOrKeyword { name: b_name, .. },
                ..
            },
        ] = &sig.parameters.value[..]
        else {
            panic!("expected two positional-or-keyword parameters");
        };
        assert_eq!(a_name, "a");
        assert_eq!(b_name, "b");
        assert_eq!(a_annotated_ty.unwrap().display(&db).to_string(), "A");
        assert_eq!(b_annotated_ty.unwrap().display(&db).to_string(), "T@f");
    }

    #[test]
    fn generic_deferred_in_stub() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.pyi",
            "
            class A: ...
            class B: ...

            alias = A

            def f[T](a: alias, b: T) -> T: ...

            alias = B
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.pyi")
            .literal(&db)
            .last_definition(&db);

        let sig = func.signature(&db);

        let [
            Parameter {
                annotated_type: a_annotated_ty,
                kind: ParameterKind::PositionalOrKeyword { name: a_name, .. },
                ..
            },
            Parameter {
                annotated_type: b_annotated_ty,
                kind: ParameterKind::PositionalOrKeyword { name: b_name, .. },
                ..
            },
        ] = &sig.parameters.value[..]
        else {
            panic!("expected two positional-or-keyword parameters");
        };
        assert_eq!(a_name, "a");
        assert_eq!(b_name, "b");
        // Parameter resolution deferred:
        assert_eq!(a_annotated_ty.unwrap().display(&db).to_string(), "A | B");
        assert_eq!(b_annotated_ty.unwrap().display(&db).to_string(), "T@f");
    }

    #[test]
    fn external_signature_no_decorator() {
        let mut db = setup_db();
        db.write_dedented(
            "/src/a.py",
            "
            def f(a: int) -> int: ...
            ",
        )
        .unwrap();
        let func = get_function_f(&db, "/src/a.py");

        let overload = func.literal(&db).last_definition(&db);
        let expected_sig = overload.signature(&db);

        // With no decorators, internal and external signature are the same
        assert_eq!(
            func.signature(&db),
            &CallableSignature::single(expected_sig)
        );
    }
}
