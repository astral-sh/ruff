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
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec_inline};

use super::{DynamicType, Type, TypeVarVariance, UnionType, semantic_index};
use crate::types::callable::CallableTypeKind;
use crate::types::constraints::{
    ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension,
};
use crate::types::generics::{
    ApplySpecialization, GenericContext, InferableTypeVars, Specialization, walk_generic_context,
};
use crate::types::infer::infer_deferred_types;
use crate::types::relation::{
    HasRelationToVisitor, IsDisjointVisitor, TypeRelation, TypeRelationChecker,
};
use crate::types::typevar::BoundTypeVarIdentity;
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance, CallableType, ErrorContext,
    FindLegacyTypeVarsVisitor, KnownClass, MaterializationKind, ParamSpecAttrKind,
    ParameterDescription, SelfBinding, TypeContext, TypeMapping, UnionBuilder, VarianceInferable,
    infer_complete_scope_types, todo_type,
};
use crate::{Db, FxOrderSet};
use ruff_python_ast::{self as ast, name::Name};
use ty_python_core::definition::Definition;

/// Infer the type of a parameter or return annotation in a function signature.
///
/// This is very similar to `definition_expression_type`, but knows that `TypeInferenceBuilder`
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
        infer_complete_scope_types(db, scope).expression_type(expression)
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

/// The per-overload information needed to synthesize one reduced signature for
/// `functools.partial(...)`.
#[derive(Clone, Debug)]
pub(crate) struct PartialSignatureApplication<'db> {
    signature: Signature<'db>,
    partial_application: PartialApplication<'db>,
    specialization: Option<Specialization<'db>>,
    unspecialized_return_ty: Type<'db>,
}

impl<'db> PartialSignatureApplication<'db> {
    /// Creates a new per-overload partial-application summary.
    pub(crate) fn new(
        signature: Signature<'db>,
        partial_application: PartialApplication<'db>,
        specialization: Option<Specialization<'db>>,
        unspecialized_return_ty: Type<'db>,
    ) -> Self {
        Self {
            signature,
            partial_application,
            specialization,
            unspecialized_return_ty,
        }
    }
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

    /// Returns the union of all overload return types, or `Unknown` if there are no overloads.
    pub(crate) fn overload_return_type_or_unknown(&self, db: &'db dyn Db) -> Type<'db> {
        match self.overloads.as_slice() {
            [] => Type::unknown(),
            [signature] => signature.return_ty,
            overloads => UnionType::from_elements(db, overloads.iter().map(|sig| sig.return_ty)),
        }
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

    /// Returns the reduced overloaded signature exposed by a `functools.partial(...)` object.
    pub(crate) fn partially_apply(
        db: &'db dyn Db,
        overloads: impl IntoIterator<Item = PartialSignatureApplication<'db>>,
    ) -> Option<Self> {
        let mut new_overloads = Vec::new();
        let mut seen_overloads = FxHashSet::default();

        for overload in overloads {
            let signature = overload.signature.partially_apply(
                db,
                &overload.partial_application,
                overload.specialization,
                overload.unspecialized_return_ty,
            );
            let dedup_key = signature.clone().with_definition(None);
            if seen_overloads.insert(dedup_key) {
                new_overloads.push(signature);
            }
        }

        (!new_overloads.is_empty()).then(|| Self::from_overloads(new_overloads))
    }

    pub(crate) fn cycle_normalized(
        &self,
        db: &'db dyn Db,
        previous: &Self,
        cycle: &salsa::Cycle,
    ) -> Self {
        if previous.overloads.len() == self.overloads.len() {
            Self {
                overloads: self
                    .overloads
                    .iter()
                    .zip(previous.overloads.iter())
                    .map(|(curr, prev)| curr.cycle_normalized(db, prev, cycle))
                    .collect(),
            }
        } else {
            debug_assert_eq!(previous, &Self::bottom());
            self.clone()
        }
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
                        return_ty: self_signature.return_ty.apply_type_mapping_impl(
                            db,
                            type_mapping,
                            tcx,
                            visitor,
                        ),
                    }))
                }
                Type::Callable(callable)
                    if matches!(callable.kind(db), CallableTypeKind::ParamSpecValue) =>
                {
                    Some(CallableSignature::from_overloads(
                        callable.signatures(db).iter().map(|signature| Signature {
                            generic_context: GenericContext::merge_optional(
                                db,
                                signature.generic_context,
                                self_signature.generic_context.map(|context| {
                                    type_mapping.update_signature_generic_context(db, context)
                                }),
                            ),
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
                            return_ty: self_signature.return_ty.apply_type_mapping_impl(
                                db,
                                type_mapping,
                                tcx,
                                visitor,
                            ),
                        }),
                    ))
                }
                _ => None,
            }
        }

        if let TypeMapping::ApplySpecialization(specialization)
        | TypeMapping::ApplySpecializationWithMaterialization { specialization, .. } =
            type_mapping
        {
            Self::from_overloads(self.overloads.iter().flat_map(|signature| {
                if let Some((prefix, paramspec)) = signature.parameters.as_paramspec_with_prefix()
                    && let Some(value) = specialization.get(db, paramspec)
                    && let Some(result) = try_apply_type_mapping_for_paramspec(
                        db,
                        signature,
                        prefix,
                        value,
                        type_mapping,
                        tcx,
                        visitor,
                    )
                {
                    result.overloads
                } else {
                    smallvec_inline![signature.apply_type_mapping_impl(
                        db,
                        type_mapping,
                        tcx,
                        visitor
                    )]
                }
            }))
        } else {
            Self::from_overloads(
                self.overloads.iter().map(|signature| {
                    signature.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                }),
            )
        }
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

    pub(crate) fn is_single_paramspec(&self) -> Option<(BoundTypeVarInstance<'db>, Type<'db>)> {
        Self::signatures_is_single_paramspec(&self.overloads)
    }

    /// Checks whether the given slice contains a single signature, and that signature is a
    /// `ParamSpec` signature. If so, returns the [`BoundTypeVarInstance`] for the `ParamSpec`,
    /// along with the return type of the signature.
    fn signatures_is_single_paramspec(
        signatures: &[Signature<'db>],
    ) -> Option<(BoundTypeVarInstance<'db>, Type<'db>)> {
        // TODO: This might need updating once we support `Concatenate`
        let [signature] = signatures else {
            return None;
        };
        signature
            .parameters
            .as_paramspec()
            .map(|bound_typevar| (bound_typevar, signature.return_ty))
    }

    pub(crate) fn when_constraint_set_assignable_to<'c>(
        &self,
        db: &'db dyn Db,
        other: &Self,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        let checker = TypeRelationChecker::constraint_set_assignability(
            constraints,
            &relation_visitor,
            &disjointness_visitor,
            &materialization_visitor,
        );
        checker.check_callable_signature_pair_inner(db, &self.overloads, &other.overloads)
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

    /// Return type. If no annotation was provided, this is `Unknown`.
    pub(crate) return_ty: Type<'db>,
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
        visitor.visit_type(db, parameter.annotated_type());
    }
    visitor.visit_type(db, signature.return_ty);
}

/// Describes how a `functools.partial(...)` call binds one overload's parameters.
///
/// `call/bind.rs` computes this from argument matching. Signature rewriting then consumes this
/// summary to synthesize the reduced callable that a partial object exposes.
#[derive(Clone, Debug)]
pub(crate) struct PartialApplication<'db> {
    positionally_bound: Box<[bool]>,
    keyword_defaults: Box<[Option<Type<'db>>]>,
    keyword_bound: Box<[bool]>,
}

impl<'db> PartialApplication<'db> {
    /// Creates an empty partial-application summary for a signature with `parameter_count`
    /// parameters.
    pub(crate) fn new(parameter_count: usize) -> Self {
        Self {
            positionally_bound: vec![false; parameter_count].into_boxed_slice(),
            keyword_defaults: vec![None; parameter_count].into_boxed_slice(),
            keyword_bound: vec![false; parameter_count].into_boxed_slice(),
        }
    }

    /// Marks the parameter at `parameter_index` as consumed by a positional binding.
    pub(crate) fn bind_positionally(&mut self, parameter_index: usize) {
        self.positionally_bound[parameter_index] = true;
    }

    /// Marks the parameter at `parameter_index` as bound by keyword and records the synthesized
    /// default type that should appear in the reduced signature, if any.
    pub(crate) fn bind_by_keyword(
        &mut self,
        parameter_index: usize,
        default_ty: Option<Type<'db>>,
    ) {
        self.keyword_bound[parameter_index] = true;
        self.keyword_defaults[parameter_index] = default_ty;
    }

    /// Returns `true` if the parameter at `parameter_index` is removed from the reduced signature
    /// because it was already supplied positionally to `functools.partial(...)`.
    pub(crate) fn is_positionally_bound(&self, parameter_index: usize) -> bool {
        self.positionally_bound[parameter_index]
    }

    fn keyword_default(&self, parameter_index: usize) -> Option<Type<'db>> {
        self.keyword_defaults[parameter_index]
    }

    fn is_keyword_bound(&self, parameter_index: usize) -> bool {
        self.keyword_bound[parameter_index]
    }
}

impl<'db> Signature<'db> {
    pub(crate) fn new(parameters: Parameters<'db>, return_ty: Type<'db>) -> Self {
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
        return_ty: Type<'db>,
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
            return_ty: signature_type,
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
            .map(|returns| function_signature_expression_type(db, definition, returns.as_ref()))
            .unwrap_or_else(Type::unknown);
        let legacy_generic_context =
            GenericContext::from_function_params(db, definition, &parameters, return_ty);
        let full_generic_context = GenericContext::merge_pep695_and_legacy(
            db,
            pep695_generic_context,
            legacy_generic_context,
        );

        // Look for any typevars bound by this function that are only mentioned in a Callable
        // return type. (We do this after merging the legacy and PEP-695 contexts because we need
        // to apply this heuristic to PEP-695 typevars as well.)
        let (generic_context, return_ty) = GenericContext::remove_callable_only_typevars(
            db,
            full_generic_context,
            &parameters,
            return_ty,
            definition,
        );

        Self {
            generic_context,
            definition: Some(definition),
            parameters,
            return_ty,
        }
    }

    pub(super) fn wrap_coroutine_return_type(self, db: &'db dyn Db) -> Self {
        let return_ty = KnownClass::CoroutineType
            .to_specialized_instance(db, &[Type::any(), Type::any(), self.return_ty]);
        Self { return_ty, ..self }
    }

    /// Returns the signature which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown() -> Self {
        Self::new(Parameters::unknown(), Type::unknown())
    }

    /// Return the "bottom" signature, subtype of all other fully-static signatures.
    pub(crate) fn bottom() -> Self {
        Self::new(Parameters::bottom(), Type::Never)
    }

    /// Returns `true` if `Self` should be hidden from the generic context display.
    ///
    /// `Self` is hidden if it does not appear in:
    /// 1. The return type
    /// 2. Any explicitly annotated parameter (not inferred)
    pub(crate) fn should_hide_self_from_display(&self, db: &'db dyn Db) -> bool {
        !self.return_ty.contains_self(db)
            && !self
                .parameters()
                .iter()
                .any(|p| p.should_annotation_be_displayed() && p.annotated_type().contains_self(db))
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

    fn cycle_normalized(&self, db: &'db dyn Db, previous: &Self, cycle: &salsa::Cycle) -> Self {
        let return_ty = self
            .return_ty
            .cycle_normalized(db, previous.return_ty, cycle);

        let parameters = if self.parameters.len() == previous.parameters.len() {
            Parameters::new(
                db,
                self.parameters
                    .iter()
                    .zip(previous.parameters.iter())
                    .map(|(curr, prev)| curr.cycle_normalized(db, prev, cycle)),
            )
        } else {
            debug_assert_eq!(previous.parameters, Parameters::bottom());
            self.parameters.clone()
        };

        Self {
            generic_context: self.generic_context,
            definition: self.definition,
            parameters,
            return_ty,
        }
    }

    pub(super) fn recursive_type_normalized_impl(
        &self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let return_ty = if nested {
            self.return_ty
                .recursive_type_normalized_impl(db, div, true)?
        } else {
            self.return_ty
                .recursive_type_normalized_impl(db, div, true)
                .unwrap_or(div)
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
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
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
            param.annotated_type().find_legacy_typevars_impl(
                db,
                binding_context,
                typevars,
                visitor,
            );
            if let Some(ty) = param.default_type() {
                ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
            }
        }
        self.return_ty
            .find_legacy_typevars_impl(db, binding_context, typevars, visitor);
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
            && first_parameter.annotated_type.is_unknown()
            && first_parameter.inferred_annotation
            && let Some(self_type) = self_type()
        {
            first_parameter.annotated_type = self_type;

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
            let self_mapping =
                TypeMapping::BindSelf(SelfBinding::new(db, self_type, binding_context));
            parameters = parameters.apply_type_mapping_impl(
                db,
                &self_mapping,
                TypeContext::default(),
                &ApplyTypeMappingVisitor::default(),
            );
            return_ty = return_ty.apply_type_mapping(db, &self_mapping, TypeContext::default());
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
        let self_mapping = TypeMapping::BindSelf(SelfBinding::new(
            db,
            self_type,
            self.definition.map(BindingContext::Definition),
        ));
        let parameters = self.parameters.apply_type_mapping_impl(
            db,
            &self_mapping,
            TypeContext::default(),
            &ApplyTypeMappingVisitor::default(),
        );
        let return_ty =
            self.return_ty
                .apply_type_mapping(db, &self_mapping, TypeContext::default());
        Self {
            generic_context: self.generic_context,
            definition: self.definition,
            parameters,
            return_ty,
        }
    }

    /// Returns this signature with the given specialization applied to parameters and return type.
    pub(crate) fn apply_specialization(
        &self,
        db: &'db dyn Db,
        specialization: Specialization<'db>,
    ) -> Self {
        let type_mapping =
            TypeMapping::ApplySpecialization(ApplySpecialization::Specialization(specialization));
        self.apply_type_mapping_impl(
            db,
            &type_mapping,
            TypeContext::default(),
            &ApplyTypeMappingVisitor::default(),
        )
    }

    /// Returns the callable signature produced by partially applying this signature.
    pub(crate) fn partially_apply(
        &self,
        db: &'db dyn Db,
        partial_application: &PartialApplication<'db>,
        specialization: Option<Specialization<'db>>,
        unspecialized_return_ty: Type<'db>,
    ) -> Self {
        let signature_specialization =
            self.partial_application_specialization(db, partial_application, specialization);
        let signature = signature_specialization.map_or_else(
            || self.clone(),
            |specialization| self.apply_specialization(db, specialization),
        );

        let parameters = signature.parameters().as_slice();
        let return_ty = specialization.map_or_else(
            || unspecialized_return_ty,
            |specialization| {
                unspecialized_return_ty
                    .apply_specialization(db, signature_specialization.unwrap_or(specialization))
            },
        );

        let mut remaining = Vec::with_capacity(parameters.len());
        let mut first_keyword_bound_positional_or_keyword = None;
        for (index, parameter) in parameters.iter().enumerate() {
            if partial_application.is_positionally_bound(index) {
                continue;
            }

            let parameter = partial_application.keyword_default(index).map_or_else(
                || parameter.clone(),
                |default_ty| parameter.clone().with_default_type(default_ty),
            );

            if first_keyword_bound_positional_or_keyword.is_none()
                && partial_application.is_keyword_bound(index)
                && matches!(parameter.kind(), ParameterKind::PositionalOrKeyword { .. })
            {
                first_keyword_bound_positional_or_keyword = Some(remaining.len());
            }

            remaining.push(parameter);
        }

        // Expand `P.args`/`P.kwargs` while the pair is still adjacent. The keyword-only reshuffle
        // below can separate them, which would otherwise prevent expansion.
        let remaining = Parameters::new(db, remaining).expand_paramspec_variadics(db);

        let mut reordered = Vec::with_capacity(remaining.len());
        let mut keyword_only = Vec::new();
        let mut keyword_variadic = Vec::new();
        for (index, parameter) in remaining.iter().cloned().enumerate() {
            let parameter = if first_keyword_bound_positional_or_keyword
                .is_some_and(|first_bound_index| index >= first_bound_index)
                && matches!(parameter.kind(), ParameterKind::PositionalOrKeyword { .. })
            {
                parameter.positional_or_keyword_to_keyword_only()
            } else {
                parameter
            };

            if parameter.is_keyword_variadic() {
                keyword_variadic.push(parameter);
            } else if parameter.is_keyword_only() {
                keyword_only.push(parameter);
            } else {
                reordered.push(parameter);
            }
        }

        reordered.extend(keyword_only);
        reordered.extend(keyword_variadic);

        signature
            .with_parameters(Parameters::new(db, reordered))
            .with_return_type(return_ty)
    }

    /// Returns the specialization used for the callable signature exposed by a partial object.
    ///
    /// Surviving type variables that still appear in the reduced parameter list may need a more
    /// specific specialization than the plain return-type view.
    fn partial_application_specialization(
        &self,
        db: &'db dyn Db,
        partial_application: &PartialApplication<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Option<Specialization<'db>> {
        let specialization = specialization?;
        let Some(generic_context) = self.generic_context else {
            return Some(specialization);
        };

        let promoted_typevars: FxHashSet<BoundTypeVarIdentity<'db>> = generic_context
            .variables(db)
            .filter(|typevar| {
                self.parameters
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| !partial_application.is_positionally_bound(*index))
                    .any(|(_, parameter)| {
                        parameter
                            .annotated_type()
                            .references_typevar(db, typevar.typevar(db).identity(db))
                    })
            })
            .map(|typevar| typevar.identity(db))
            .collect();

        if promoted_typevars.is_empty() {
            return Some(specialization);
        }

        Some(generic_context.specialize_recursive(
            db,
            generic_context.variables(db).map(|typevar| {
                let ty = specialization
                    .get(db, typevar)
                    .unwrap_or(Type::TypeVar(typevar));
                Some(if promoted_typevars.contains(&typevar.identity(db)) {
                    ty.promote(db)
                } else {
                    ty
                })
            }),
        ))
    }

    fn inferable_typevars(&self, db: &'db dyn Db) -> InferableTypeVars<'db> {
        match self.generic_context {
            Some(generic_context) => generic_context.inferable_typevars(db),
            None => InferableTypeVars::None,
        }
    }

    pub(crate) fn when_constraint_set_assignable_to_signatures<'c>(
        &self,
        db: &'db dyn Db,
        other: &CallableSignature<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        // If this signature is a paramspec, bind it to the entire overloaded other callable.
        if let Some(self_bound_typevar) = self.parameters.as_paramspec()
            && other.is_single_paramspec().is_none()
        {
            let upper = Type::Callable(CallableType::new(
                db,
                CallableSignature::from_overloads(other.overloads.iter().map(|signature| {
                    Signature::new_generic(
                        signature.generic_context,
                        signature.parameters().clone(),
                        Type::unknown(),
                    )
                })),
                CallableTypeKind::ParamSpecValue,
            ));
            let param_spec_matches = ConstraintSet::constrain_typevar(
                db,
                constraints,
                self_bound_typevar,
                Type::Never,
                upper,
            );
            let return_types_match = other
                .overloads
                .iter()
                .map(|signature| signature.return_ty)
                .when_any(db, constraints, |other_return_type| {
                    self.return_ty.when_constraint_set_assignable_to(
                        db,
                        other_return_type,
                        constraints,
                    )
                });
            return param_spec_matches.and(db, constraints, || return_types_match);
        }

        other
            .overloads
            .iter()
            .when_all(db, constraints, |other_signature| {
                self.when_constraint_set_assignable_to(db, other_signature, constraints)
            })
    }

    fn when_constraint_set_assignable_to<'c>(
        &self,
        db: &'db dyn Db,
        other: &Self,
        constraints: &'c ConstraintSetBuilder<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        let checker = TypeRelationChecker::constraint_set_assignability(
            constraints,
            &relation_visitor,
            &disjointness_visitor,
            &materialization_visitor,
        );
        checker.check_signature_pair(db, self, other)
    }

    /// Create a new signature with the given definition.
    pub(crate) fn with_definition(self, definition: Option<Definition<'db>>) -> Self {
        Self { definition, ..self }
    }

    /// Create a new signature with the given parameters.
    pub(crate) fn with_parameters(self, parameters: Parameters<'db>) -> Self {
        Self { parameters, ..self }
    }

    /// Create a new signature with the given return type.
    pub(crate) fn with_return_type(self, return_ty: Type<'db>) -> Self {
        Self { return_ty, ..self }
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
                    ParameterForm::Value => Some(
                        parameter
                            .annotated_type()
                            .with_polarity(TypeVarVariance::Contravariant)
                            .variance_of(db, typevar),
                    ),
                }),
            Some(self.return_ty.variance_of(db, typevar)),
        )
        .collect()
    }
}

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    /// Fast path for unary callable assignability: compare overload sets by aggregating
    /// overlapping parameter domains and return types.
    ///
    /// This is intentionally accept-only. If the probe does not definitely succeed, it returns
    /// `None` and callers should fall back to legacy per-overload relation checks.
    fn try_unary_overload_aggregate_relation(
        &self,
        db: &'db dyn Db,
        source_signatures: &[Signature<'db>],
        target_signature: &Signature<'db>,
    ) -> Option<ConstraintSet<'db, 'c>> {
        let single_required_positional_parameter_type = |signature: &Signature<'db>| {
            if signature.parameters().len() != 1 {
                return None;
            }
            let parameter = signature.parameters().get(0)?;

            match parameter.kind() {
                ParameterKind::PositionalOnly {
                    default_type: None, ..
                }
                | ParameterKind::PositionalOrKeyword {
                    default_type: None, ..
                } => Some(parameter.annotated_type()),
                _ => None,
            }
        };

        let is_unary_overload_aggregate_candidate_type = |ty: Type<'db>| {
            // Keep aggregate probing away from inference-sensitive shapes and defer them to the
            // legacy path, which already handles dynamic/typevar interactions.
            !ty.has_dynamic(db) && !ty.has_typevar_or_typevar_instance(db)
        };

        let other_parameter_type = single_required_positional_parameter_type(target_signature)?;
        // Keep this aggregate path narrowly scoped to unary target callables whose parameter
        // domain is an explicit union.
        //
        // Broader overload-set assignability (non-union unary domains, higher arity,
        // typevars/dynamic interactions) needs dedicated relation logic.
        if !matches!(other_parameter_type, Type::Union(_))
            || !is_unary_overload_aggregate_candidate_type(other_parameter_type)
            || !is_unary_overload_aggregate_candidate_type(target_signature.return_ty)
        {
            return None;
        }

        let mut parameter_type_union = UnionBuilder::new(db);
        let mut return_type_union = UnionBuilder::new(db);
        let mut has_overlapping_domain = false;

        for self_signature in source_signatures {
            let self_parameter_type = single_required_positional_parameter_type(self_signature)?;
            if !is_unary_overload_aggregate_candidate_type(self_parameter_type)
                || !is_unary_overload_aggregate_candidate_type(self_signature.return_ty)
            {
                return None;
            }
            let signatures_are_disjoint = self
                .as_disjointness_checker()
                .check_type_pair(db, self_parameter_type, other_parameter_type)
                .is_always_satisfied(db);

            if signatures_are_disjoint {
                continue;
            }

            has_overlapping_domain = true;
            parameter_type_union = parameter_type_union.add(self_parameter_type);
            return_type_union = return_type_union.add(self_signature.return_ty);
        }

        if !has_overlapping_domain {
            return None;
        }

        // Function assignability here is parameter-contravariant and return-covariant.
        let parameters_cover_target =
            self.check_type_pair(db, other_parameter_type, parameter_type_union.build());
        let returns_match_target =
            || self.check_type_pair(db, return_type_union.build(), target_signature.return_ty);
        let aggregate_relation =
            parameters_cover_target.and(db, self.constraints, returns_match_target);
        aggregate_relation
            .is_always_satisfied(db)
            .then_some(aggregate_relation)
    }

    pub(super) fn check_callable_signature_pair(
        &self,
        db: &'db dyn Db,
        source: &CallableSignature<'db>,
        target: &CallableSignature<'db>,
    ) -> ConstraintSet<'db, 'c> {
        self.check_callable_signature_pair_inner(db, &source.overloads, &target.overloads)
    }

    /// Implementation of subtyping and assignability between two, possible overloaded, callable
    /// types.
    fn check_callable_signature_pair_inner(
        &self,
        db: &'db dyn Db,
        source_overloads: &[Signature<'db>],
        target_overloads: &[Signature<'db>],
    ) -> ConstraintSet<'db, 'c> {
        if self.relation.is_constraint_set_assignability() {
            // TODO: Oof, maybe ParamSpec needs to live at CallableSignature, not Signature?
            let source_is_single_paramspec =
                CallableSignature::signatures_is_single_paramspec(source_overloads);
            let target_is_single_paramspec =
                CallableSignature::signatures_is_single_paramspec(target_overloads);

            // TODO: Adding proper support for overloads with ParamSpec will likely require some
            // changes here.

            // Only handle ParamSpec here when we still need the whole overload set. Once we're
            // down to a single signature on both sides, let
            // `TypeRelationChecker::check_signature_pair_inner` handle the ParamSpec binding
            // instead.
            match (source_is_single_paramspec, target_is_single_paramspec) {
                (Some((source_tvar, source_return)), None) if target_overloads.len() > 1 => {
                    let upper = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::from_overloads(target_overloads.iter().map(
                            |signature| {
                                Signature::new_generic(
                                    signature.generic_context,
                                    signature.parameters().clone(),
                                    Type::unknown(),
                                )
                            },
                        )),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        source_tvar,
                        Type::Never,
                        upper,
                    );
                    let return_types_match = || {
                        // TODO: Similar to how we do this for unions, we should collect error
                        // context for all elements and report it if *all* checks fail.
                        self.without_context_collection(|| {
                            target_overloads
                                .iter()
                                .map(|signature| signature.return_ty)
                                .when_any(db, self.constraints, |target_return| {
                                    self.check_type_pair(db, source_return, target_return)
                                })
                        })
                    };
                    return param_spec_matches.and(db, self.constraints, return_types_match);
                }

                (None, Some((target_tvar, target_return))) if source_overloads.len() > 1 => {
                    let lower = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::from_overloads(source_overloads.iter().map(
                            |signature| {
                                Signature::new_generic(
                                    signature.generic_context,
                                    signature.parameters().clone(),
                                    Type::unknown(),
                                )
                            },
                        )),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        target_tvar,
                        lower,
                        Type::object(),
                    );
                    let return_types_match = || {
                        // TODO: Similar to how we do this for unions, we should collect error
                        // context for all elements and report it if *all* checks fail.
                        self.without_context_collection(|| {
                            source_overloads
                                .iter()
                                .map(|signature| signature.return_ty)
                                .when_any(db, self.constraints, |source_return| {
                                    self.check_type_pair(db, source_return, target_return)
                                })
                        })
                    };
                    return param_spec_matches.and(db, self.constraints, return_types_match);
                }

                _ => {}
            }
        }

        match (source_overloads, target_overloads) {
            ([source_signature], [target_signature]) => {
                // Base case: both callable types contain a single signature.
                if self.relation.is_constraint_set_assignability()
                    && (source_signature
                        .parameters
                        .as_paramspec_with_prefix()
                        .is_some()
                        || target_signature
                            .parameters
                            .as_paramspec_with_prefix()
                            .is_some())
                {
                    self.check_signature_pair_inner(db, source_signature, target_signature)
                } else {
                    self.check_signature_pair(db, source_signature, target_signature)
                }
            }

            // source is possibly overloaded while target is definitely not overloaded.
            (_, [target_signature]) => {
                if let Some(aggregate_relation) = self.try_unary_overload_aggregate_relation(
                    db,
                    source_overloads,
                    target_signature,
                ) {
                    return aggregate_relation;
                }

                // TODO: Similar to how we do this for unions, we should collect error
                // context for all elements and report it if *all* checks fail.
                self.without_context_collection(|| {
                    source_overloads
                        .iter()
                        .when_any(db, self.constraints, |self_signature| {
                            self.check_callable_signature_pair_inner(
                                db,
                                std::slice::from_ref(self_signature),
                                target_overloads,
                            )
                        })
                })
            }

            // source is definitely not overloaded while target is possibly overloaded.
            ([_], _) => {
                target_overloads
                    .iter()
                    .when_all(db, self.constraints, |target_signature| {
                        self.check_callable_signature_pair_inner(
                            db,
                            source_overloads,
                            std::slice::from_ref(target_signature),
                        )
                    })
            }

            // source is definitely overloaded while target is possibly overloaded.
            (_, _) => target_overloads
                .iter()
                .when_all(db, self.constraints, |target_signature| {
                    self.check_callable_signature_pair_inner(
                        db,
                        source_overloads,
                        std::slice::from_ref(target_signature),
                    )
                }),
        }
    }

    /// Implementation of subtyping and assignability for signature.
    fn check_signature_pair(
        &self,
        db: &'db dyn Db,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> ConstraintSet<'db, 'c> {
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
        let source_inferable = source.inferable_typevars(db);
        let target_inferable = target.inferable_typevars(db);
        let inferable = source_inferable.merge(db, target_inferable);
        let inferable = self.inferable.merge(db, inferable);

        // `inner` will create a constraint set that references these newly inferable typevars.
        let checker = self.with_inferable_typevars(inferable);
        let when = checker.check_signature_pair_inner(db, source, target);

        // But the caller does not need to consider those extra typevars. Whatever constraint set
        // we produce, we reduce it back down to the inferable set that the caller asked about.
        // If we introduced new inferable typevars, those will be existentially quantified away
        // before returning.
        when.reduce_inferable(
            db,
            self.constraints,
            source_inferable.iter(db).chain(target_inferable.iter(db)),
        )
    }

    fn check_signature_pair_inner(
        &self,
        db: &'db dyn Db,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> ConstraintSet<'db, 'c> {
        /// A helper struct to zip two slices of parameters together that provides control over the
        /// two iterators individually. It also keeps track of the current parameter in each
        /// iterator.
        struct ParametersZip<'a, 'db> {
            current_source: Option<&'a Parameter<'db>>,
            current_target: Option<&'a Parameter<'db>>,
            source_iter: Iter<'a, Parameter<'db>>,
            target_iter: Iter<'a, Parameter<'db>>,
        }

        impl<'a, 'db> ParametersZip<'a, 'db> {
            /// Move to the next parameter in both the `source` and `target` parameter iterators,
            /// [`None`] if both iterators are exhausted.
            fn next(&mut self) -> Option<EitherOrBoth<&'a Parameter<'db>, &'a Parameter<'db>>> {
                match (self.next_source(), self.next_target()) {
                    (Some(source_param), Some(target_param)) => {
                        Some(EitherOrBoth::Both(source_param, target_param))
                    }
                    (Some(source_param), None) => Some(EitherOrBoth::Left(source_param)),
                    (None, Some(target_param)) => Some(EitherOrBoth::Right(target_param)),
                    (None, None) => None,
                }
            }

            /// Move to the next parameter in the `source` parameter iterator, [`None`] if the
            /// iterator is exhausted.
            fn next_source(&mut self) -> Option<&'a Parameter<'db>> {
                self.current_source = self.source_iter.next();
                self.current_source
            }

            /// Move to the next parameter in the `target` parameter iterator, [`None`] if the
            /// iterator is exhausted.
            fn next_target(&mut self) -> Option<&'a Parameter<'db>> {
                self.current_target = self.target_iter.next();
                self.current_target
            }

            /// Peek at the next parameter in the `target` parameter iterator without consuming it.
            fn peek_target(&mut self) -> Option<&'a Parameter<'db>> {
                self.target_iter.clone().next()
            }

            /// Consumes the `ParametersZip` and returns a two-element tuple containing the
            /// remaining parameters in the `source` and `target` iterators respectively.
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
                    self.current_source.into_iter().chain(self.source_iter),
                    self.current_target.into_iter().chain(self.target_iter),
                )
            }
        }

        let mut result = self.always();

        // Avoid returning early after checking the return types in case there is a `ParamSpec` type
        // variable in either signature to ensure that the `ParamSpec` binding is still applied even
        // if the return types are incompatible.
        let return_type_constraints = self.check_type_pair(db, source.return_ty, target.return_ty);
        let return_type_checks = !result
            .intersect(db, self.constraints, return_type_constraints)
            .is_never_satisfied(db);
        if !return_type_checks {
            self.provide_context(|| ErrorContext::IncompatibleReturnTypes {
                source: source.return_ty,
                target: target.return_ty,
            });
        }

        let mut check_types = |target_ty: Type<'db>,
                               source_ty: Type<'db>,
                               target_name: Option<&Name>,
                               target_index: usize| {
            match (target_ty, source_ty) {
                // This is a special case where the _same_ components of two different `ParamSpec`
                // type variables are assignable to each other when they're both in an inferable
                // position.
                //
                // `ParamSpec` type variables can only occur in parameter lists so this special case
                // is present here instead of in `TypeRelationChecker::check_type_pair`.
                (Type::TypeVar(typevar1), Type::TypeVar(typevar2))
                    if typevar1.paramspec_attr(db).is_some()
                        && typevar1.paramspec_attr(db) == typevar2.paramspec_attr(db)
                        && typevar1
                            .without_paramspec_attr(db)
                            .is_inferable(db, self.inferable)
                        && typevar2
                            .without_paramspec_attr(db)
                            .is_inferable(db, self.inferable) =>
                {
                    return true;
                }
                _ => {}
            }

            let constraint_set = self.check_type_pair(db, target_ty, source_ty);
            if constraint_set.is_never_satisfied(db) {
                let parameter = ParameterDescription::new(target_index, target_name);
                self.provide_context(|| ErrorContext::IncompatibleParameterTypes {
                    source: source_ty,
                    target: target_ty,
                    parameter,
                });
            }
            !result
                .intersect(db, self.constraints, constraint_set)
                .is_never_satisfied(db)
        };

        if self.relation.is_constraint_set_assignability() {
            let source_paramspec = source.parameters.as_paramspec_with_prefix();
            let target_paramspec = target.parameters.as_paramspec_with_prefix();

            // If either signature is a ParamSpec, the constraint set should bind the ParamSpec to
            // the other signature before the return-type and gradual/top fast paths can return
            // early. We also need to compare the return types here so a return-type mismatch still
            // preserves the inferred ParamSpec binding.
            match (source_paramspec, target_paramspec) {
                // self: `P`
                // other: `P`
                (Some(([], source_bound_typevar)), Some(([], target_bound_typevar))) => {
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        source_bound_typevar,
                        Type::TypeVar(target_bound_typevar),
                        Type::TypeVar(target_bound_typevar),
                    );
                    result.intersect(db, self.constraints, param_spec_matches);
                    return result;
                }

                // self: `Concatenate[<prefix_params>, P]`
                // other: `P`
                (
                    Some((source_prefix_params, source_bound_typevar)),
                    Some(([], target_bound_typevar)),
                ) => {
                    let lower = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new_generic(
                            source.generic_context,
                            Parameters::concatenate(
                                db,
                                source_prefix_params.to_vec(),
                                ConcatenateTail::ParamSpec(source_bound_typevar),
                            ),
                            Type::unknown(),
                        )),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_prefix_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        target_bound_typevar,
                        lower,
                        Type::object(),
                    );
                    result.intersect(db, self.constraints, param_spec_prefix_matches);
                    return result;
                }

                // self: `P`
                // other: `Concatenate[<prefix_params>, P]`
                (
                    Some(([], source_bound_typevar)),
                    Some((target_prefix_params, target_bound_typevar)),
                ) => {
                    let upper = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new_generic(
                            target.generic_context,
                            Parameters::concatenate(
                                db,
                                target_prefix_params.to_vec(),
                                ConcatenateTail::ParamSpec(target_bound_typevar),
                            ),
                            Type::unknown(),
                        )),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        source_bound_typevar,
                        Type::Never,
                        upper,
                    );
                    result.intersect(db, self.constraints, param_spec_matches);
                    return result;
                }

                // self: `Concatenate[<prefix_params>, P]`
                // other: `Concatenate[<prefix_params>, P]`
                (
                    Some((source_prefix_params, source_bound_typevar)),
                    Some((target_prefix_params, target_bound_typevar)),
                ) => {
                    let mut parameters = ParametersZip {
                        current_source: None,
                        current_target: None,
                        source_iter: source_prefix_params.iter(),
                        target_iter: target_prefix_params.iter(),
                    };

                    // Note that in the following loop, the `Concatenate` case could come from a
                    // regular function signature like:
                    //
                    // ```python
                    // def test[**P](fn: Callable[P, None], /, x: int, *args: P.args, **kwargs: P.kwargs) -> None: ...
                    // ```
                    //
                    // Here, `fn` is positional-only parameter because of the `/` while `x` is a
                    // positional-or-keyword parameter.

                    let mut target_index = 0usize;
                    while let Some(EitherOrBoth::Both(source_param, target_param)) =
                        parameters.next()
                    {
                        match (source_param.kind(), target_param.kind()) {
                            (
                                ParameterKind::PositionalOnly {
                                    default_type: source_default,
                                    ..
                                }
                                | ParameterKind::PositionalOrKeyword {
                                    default_type: source_default,
                                    ..
                                },
                                ParameterKind::PositionalOnly {
                                    default_type: other_default,
                                    ..
                                },
                            ) => {
                                if source_default.is_none() && other_default.is_some() {
                                    return self.never();
                                }
                                if !check_types(
                                    target_param.annotated_type(),
                                    source_param.annotated_type(),
                                    target_param.name(),
                                    target_index,
                                ) {
                                    return result;
                                }
                            }

                            (
                                ParameterKind::PositionalOrKeyword {
                                    name: self_name,
                                    default_type: source_default,
                                },
                                ParameterKind::PositionalOrKeyword {
                                    name: other_name,
                                    default_type: other_default,
                                },
                            ) => {
                                if self_name != other_name {
                                    self.provide_context(|| ErrorContext::ParameterNameMismatch {
                                        source_name: self_name.clone(),
                                        target_name: other_name.clone(),
                                    });
                                    return self.never();
                                }
                                // The following checks are the same as positional-only parameters.
                                if source_default.is_none() && other_default.is_some() {
                                    return self.never();
                                }
                                if !check_types(
                                    target_param.annotated_type(),
                                    source_param.annotated_type(),
                                    target_param.name(),
                                    target_index,
                                ) {
                                    return result;
                                }
                            }

                            _ => return self.never(),
                        }
                        target_index += 1;
                    }

                    let (mut source_params, mut target_params) = parameters.into_remaining();

                    // At this point, we should've exhausted at least one of the parameter lists,
                    // so only one side can have remaining prefix parameters.
                    if let Some(source_param) = source_params.next() {
                        let lower = Type::Callable(CallableType::new(
                            db,
                            CallableSignature::single(Signature::new_generic(
                                source.generic_context,
                                Parameters::concatenate(
                                    db,
                                    std::iter::once(source_param.clone())
                                        .chain(source_params.cloned())
                                        .collect(),
                                    ConcatenateTail::ParamSpec(source_bound_typevar),
                                ),
                                Type::unknown(),
                            )),
                            CallableTypeKind::ParamSpecValue,
                        ));
                        let param_spec_prefix_matches = ConstraintSet::constrain_typevar(
                            db,
                            self.constraints,
                            target_bound_typevar,
                            lower,
                            Type::object(),
                        );
                        result.intersect(db, self.constraints, param_spec_prefix_matches);
                    } else if let Some(target_param) = target_params.next() {
                        let upper = Type::Callable(CallableType::new(
                            db,
                            CallableSignature::single(Signature::new_generic(
                                target.generic_context,
                                Parameters::concatenate(
                                    db,
                                    std::iter::once(target_param.clone())
                                        .chain(target_params.cloned())
                                        .collect(),
                                    ConcatenateTail::ParamSpec(target_bound_typevar),
                                ),
                                Type::unknown(),
                            )),
                            CallableTypeKind::ParamSpecValue,
                        ));
                        let param_spec_prefix_matches = ConstraintSet::constrain_typevar(
                            db,
                            self.constraints,
                            source_bound_typevar,
                            Type::Never,
                            upper,
                        );
                        result.intersect(db, self.constraints, param_spec_prefix_matches);
                    } else {
                        // When the prefixes match exactly, we just relate the remaining tails.
                        let param_spec_matches = ConstraintSet::constrain_typevar(
                            db,
                            self.constraints,
                            source_bound_typevar,
                            Type::TypeVar(target_bound_typevar),
                            Type::TypeVar(target_bound_typevar),
                        );
                        result.intersect(db, self.constraints, param_spec_matches);
                    }
                    return result;
                }

                // self: callable without ParamSpec
                // other: `P`
                (None, Some(([], target_bound_typevar))) => {
                    let lower = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new_generic(
                            source.generic_context,
                            source.parameters.clone(),
                            Type::unknown(),
                        )),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        target_bound_typevar,
                        lower,
                        Type::object(),
                    );
                    result.intersect(db, self.constraints, param_spec_matches);
                    return result;
                }

                // self: callable without ParamSpec
                // other: `Concatenate[<prefix_params>, P]`
                (None, Some((target_prefix_params, target_bound_typevar))) => {
                    // Loop over self parameters and target_prefix_params in a similar manner to the
                    // above loop
                    let mut parameters = ParametersZip {
                        current_source: None,
                        current_target: None,
                        source_iter: source.parameters.iter(),
                        target_iter: target_prefix_params.iter(),
                    };

                    let mut target_index = 0usize;
                    while let Some(next_parameter) = parameters.next() {
                        match next_parameter {
                            EitherOrBoth::Left(_) => {
                                // If the non-Concatenate callable has remaining parameters, they
                                // should be bound to the `ParamSpec` in other.
                                break;
                            }
                            EitherOrBoth::Right(_) => {
                                return self.never();
                            }
                            EitherOrBoth::Both(source_param, target_param) => {
                                match (source_param.kind(), target_param.kind()) {
                                    (
                                        ParameterKind::PositionalOnly {
                                            default_type: source_default,
                                            ..
                                        }
                                        | ParameterKind::PositionalOrKeyword {
                                            default_type: source_default,
                                            ..
                                        },
                                        ParameterKind::PositionalOnly {
                                            default_type: target_default,
                                            ..
                                        },
                                    ) => {
                                        if source_default.is_none() && target_default.is_some() {
                                            return self.never();
                                        }
                                        if !check_types(
                                            target_param.annotated_type(),
                                            source_param.annotated_type(),
                                            target_param.name(),
                                            target_index,
                                        ) {
                                            return result;
                                        }
                                    }

                                    (
                                        ParameterKind::PositionalOrKeyword {
                                            name: source_name,
                                            default_type: source_default,
                                        },
                                        ParameterKind::PositionalOrKeyword {
                                            name: target_name,
                                            default_type: target_default,
                                        },
                                    ) => {
                                        if source_name != target_name {
                                            return self.never();
                                        }
                                        // The following checks are the same as positional-only parameters.
                                        if source_default.is_none() && target_default.is_some() {
                                            return self.never();
                                        }
                                        if !check_types(
                                            target_param.annotated_type(),
                                            source_param.annotated_type(),
                                            target_param.name(),
                                            target_index,
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
                                            target_param.annotated_type(),
                                            source_param.annotated_type(),
                                            target_param.name(),
                                            target_index,
                                        ) {
                                            return result;
                                        }

                                        while let Some(target_param) = parameters.peek_target() {
                                            target_index += 1;
                                            if !check_types(
                                                target_param.annotated_type(),
                                                source_param.annotated_type(),
                                                target_param.name(),
                                                target_index,
                                            ) {
                                                return result;
                                            }
                                            parameters.next_target();
                                        }

                                        break;
                                    }

                                    _ => return self.never(),
                                }
                            }
                        }
                        target_index += 1;
                    }

                    let (source_params, _) = parameters.into_remaining();
                    let lower = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new_generic(
                            source.generic_context,
                            Parameters::new(db, source_params.cloned()),
                            Type::unknown(),
                        )),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_prefix_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        target_bound_typevar,
                        lower,
                        Type::object(),
                    );
                    result.intersect(db, self.constraints, param_spec_prefix_matches);

                    return result;
                }

                // self: `P`
                // other: callable without ParamSpec
                (Some(([], source_bound_typevar)), None) => {
                    let upper = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new_generic(
                            target.generic_context,
                            target.parameters.clone(),
                            Type::unknown(),
                        )),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        source_bound_typevar,
                        Type::Never,
                        upper,
                    );
                    result.intersect(db, self.constraints, param_spec_matches);
                    return result;
                }

                // self: `Concatenate[<prefix_params>, P]`
                // other: callable without ParamSpec
                (Some((source_prefix_params, source_bound_typevar)), None) => {
                    let mut parameters = ParametersZip {
                        current_source: None,
                        current_target: None,
                        source_iter: source_prefix_params.iter(),
                        target_iter: target.parameters.iter(),
                    };

                    if target.parameters.kind() != ParametersKind::Gradual {
                        let mut target_index = 0usize;
                        while let Some(next_parameter) = parameters.next() {
                            match next_parameter {
                                EitherOrBoth::Left(_) => {
                                    return self.never();
                                }
                                EitherOrBoth::Right(_) => {
                                    // If the non-Concatenate callable has remaining parameters, they
                                    // should be bound to the `ParamSpec` in self.
                                    break;
                                }
                                EitherOrBoth::Both(source_param, target_param) => {
                                    match (source_param.kind(), target_param.kind()) {
                                        (
                                            ParameterKind::PositionalOnly {
                                                default_type: source_default,
                                                ..
                                            }
                                            | ParameterKind::PositionalOrKeyword {
                                                default_type: source_default,
                                                ..
                                            },
                                            ParameterKind::PositionalOnly {
                                                default_type: target_default,
                                                ..
                                            },
                                        ) => {
                                            if source_default.is_none() && target_default.is_some()
                                            {
                                                return self.never();
                                            }
                                            if !check_types(
                                                target_param.annotated_type(),
                                                source_param.annotated_type(),
                                                target_param.name(),
                                                target_index,
                                            ) {
                                                return result;
                                            }
                                        }

                                        (
                                            ParameterKind::PositionalOrKeyword {
                                                name: source_name,
                                                default_type: source_default,
                                            },
                                            ParameterKind::PositionalOrKeyword {
                                                name: target_name,
                                                default_type: target_default,
                                            },
                                        ) => {
                                            if source_name != target_name {
                                                return self.never();
                                            }
                                            // The following checks are the same as positional-only parameters.
                                            if source_default.is_none() && target_default.is_some()
                                            {
                                                return self.never();
                                            }
                                            if !check_types(
                                                target_param.annotated_type(),
                                                source_param.annotated_type(),
                                                target_param.name(),
                                                target_index,
                                            ) {
                                                return result;
                                            }
                                        }

                                        _ => return self.never(),
                                    }
                                }
                            }
                            target_index += 1;
                        }
                    }

                    let (_, target_params) = parameters.into_remaining();
                    let upper = Type::Callable(CallableType::new(
                        db,
                        CallableSignature::single(Signature::new_generic(
                            target.generic_context,
                            Parameters::new(db, target_params.cloned()),
                            Type::unknown(),
                        )),
                        CallableTypeKind::ParamSpecValue,
                    ));
                    let param_spec_prefix_matches = ConstraintSet::constrain_typevar(
                        db,
                        self.constraints,
                        source_bound_typevar,
                        Type::Never,
                        upper,
                    );
                    result.intersect(db, self.constraints, param_spec_prefix_matches);

                    return result;
                }

                // Both self and other are callables without ParamSpecs
                (None, None) => {}
            }
        }

        if !return_type_checks {
            return result;
        }

        // A gradual parameter list is a supertype of the "bottom" parameter list (*args: object,
        // **kwargs: object).
        if target.parameters.is_gradual()
            && !source.parameters.is_top()
            && source
                .parameters
                .variadic()
                .is_some_and(|(_, param)| param.annotated_type().is_object())
            && source
                .parameters
                .keyword_variadic()
                .is_some_and(|(_, param)| param.annotated_type().is_object())
        {
            return self.always();
        }

        // The top signature is supertype of (and assignable from) all other signatures. It is a
        // subtype of no signature except itself, and assignable only to the gradual signature.
        if target.parameters.is_top() {
            return self.always();
        } else if source.parameters.is_top() && !target.parameters.is_gradual() {
            return self.never();
        }

        // If either of the parameter lists is gradual (`...`), then it is assignable to and from
        // any other parameter list, but not a subtype or supertype of any other parameter list.
        if source.parameters.is_gradual() || target.parameters.is_gradual() {
            match (source.parameters.kind(), target.parameters.kind()) {
                // Both parameter lists are `Concatenate` with gradual forms. All prefix parameters
                // are going to be positional-only.
                (
                    ParametersKind::Concatenate(ConcatenateTail::Gradual),
                    ParametersKind::Concatenate(ConcatenateTail::Gradual),
                ) => {
                    let source_prefix_params =
                        &source.parameters.value[..source.parameters.len().saturating_sub(2)];
                    let target_prefix_params =
                        &target.parameters.value[..target.parameters.len().saturating_sub(2)];

                    for (target_index, (source_param, target_param)) in source_prefix_params
                        .iter()
                        .zip(target_prefix_params.iter())
                        .enumerate()
                    {
                        if !check_types(
                            target_param.annotated_type(),
                            source_param.annotated_type(),
                            target_param.name(),
                            target_index,
                        ) {
                            return result;
                        }
                    }
                }

                // Self is a `Concatenate` with gradual form while other is a regular non-gradual
                // callable
                (
                    ParametersKind::Concatenate(ConcatenateTail::Gradual),
                    ParametersKind::Standard,
                ) => {
                    let source_prefix_params =
                        &source.parameters.value[..source.parameters.len().saturating_sub(2)];

                    for (target_index, param) in source_prefix_params
                        .iter()
                        .zip_longest(target.parameters.iter())
                        .enumerate()
                    {
                        match param {
                            EitherOrBoth::Left(_) => {
                                // Concatenate (self) has additional positional-only parameters but
                                // other does not.
                                return self.never();
                            }
                            EitherOrBoth::Right(_) => {
                                // Once the left (self) iterator is exhausted, all the remaining
                                // parameters in other will be consumed by the gradual form of
                                // `Concatenate`.
                                break;
                            }
                            EitherOrBoth::Both(source_param, target_param) => {
                                if let (
                                    ParameterKind::PositionalOnly { .. },
                                    ParameterKind::PositionalOnly {
                                        default_type: target_default,
                                        ..
                                    },
                                ) = (source_param.kind(), target_param.kind())
                                {
                                    // `self`'s default is always going to be `None` because it comes
                                    // from the `Concatenate` form which cannot have default value.
                                    if target_default.is_some() {
                                        return self.never();
                                    }
                                    if !check_types(
                                        target_param.annotated_type(),
                                        source_param.annotated_type(),
                                        target_param.name(),
                                        target_index,
                                    ) {
                                        return result;
                                    }
                                } else {
                                    return self.never();
                                }
                            }
                        }
                    }
                }

                // Other is a `Concatenate` with gradual form while self is a regular non-gradual
                // callable
                (
                    ParametersKind::Standard,
                    ParametersKind::Concatenate(ConcatenateTail::Gradual),
                ) => {
                    let target_prefix_params =
                        &target.parameters.value[..target.parameters.len().saturating_sub(2)];

                    let mut parameters = ParametersZip {
                        current_source: None,
                        current_target: None,
                        source_iter: source.parameters.iter(),
                        target_iter: target_prefix_params.iter(),
                    };

                    let mut target_index = 0usize;
                    while let Some(parameter) = parameters.next() {
                        match parameter {
                            EitherOrBoth::Left(_) => {
                                // Once the right (other) iterator is exhausted, all the remaining
                                // parameters in self will be consumed by the gradual form of
                                // `Concatenate`.
                                break;
                            }
                            EitherOrBoth::Right(_) => {
                                // Concatenate (other) has additional positional-only parameters but
                                // self does not.
                                return self.never();
                            }
                            EitherOrBoth::Both(source_param, target_param) => {
                                match source_param.kind() {
                                    ParameterKind::PositionalOnly {
                                        default_type: source_default,
                                        ..
                                    }
                                    | ParameterKind::PositionalOrKeyword {
                                        default_type: source_default,
                                        ..
                                    } => {
                                        if source_default.is_none()
                                            && target_param.default_type().is_some()
                                        {
                                            return self.never();
                                        }
                                        if !check_types(
                                            target_param.annotated_type(),
                                            source_param.annotated_type(),
                                            target_param.name(),
                                            target_index,
                                        ) {
                                            return result;
                                        }
                                    }
                                    ParameterKind::Variadic { .. } => {
                                        if !check_types(
                                            target_param.annotated_type(),
                                            source_param.annotated_type(),
                                            target_param.name(),
                                            target_index,
                                        ) {
                                            return result;
                                        }

                                        while let Some(target_param) = parameters.peek_target() {
                                            target_index += 1;
                                            if !check_types(
                                                target_param.annotated_type(),
                                                source_param.annotated_type(),
                                                target_param.name(),
                                                target_index,
                                            ) {
                                                return result;
                                            }
                                            parameters.next_target();
                                        }
                                    }
                                    _ => {
                                        // self has other parameter kinds but other only has
                                        // positional-only parameters, so they cannot be compatible.
                                        return self.never();
                                    }
                                }
                            }
                        }
                        target_index += 1;
                    }
                }

                _ => {}
            }

            return match self.relation {
                TypeRelation::Subtyping | TypeRelation::SubtypingAssuming => self.never(),
                TypeRelation::Redundancy { .. } => result.intersect(
                    db,
                    self.constraints,
                    ConstraintSet::from_bool(
                        self.constraints,
                        source.parameters.is_gradual() && target.parameters.is_gradual(),
                    ),
                ),
                TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability => result,
            };
        }

        let mut parameters = ParametersZip {
            current_source: None,
            current_target: None,
            source_iter: source.parameters.iter(),
            target_iter: target.parameters.iter(),
        };

        // Collect all the standard parameters that have only been matched against a variadic
        // parameter which means that the keyword variant is still unmatched.
        let mut target_keywords = Vec::new();
        let mut target_index = 0usize;

        loop {
            let Some(next_parameter) = parameters.next() else {
                if target_keywords.is_empty() {
                    // All parameters have been checked or both the parameter lists were empty.
                    // In either case, `source` is a subtype of `target`.
                    return result;
                }
                // There are keyword parameters in `target` that were only matched positionally
                // against a variadic parameter in `source`. We need to verify that they can also
                // be matched as keyword arguments, which is done after this loop.
                break;
            };

            match next_parameter {
                EitherOrBoth::Left(source_parameter) => match source_parameter.kind() {
                    ParameterKind::KeywordOnly { .. } | ParameterKind::KeywordVariadic { .. }
                        if !target_keywords.is_empty() =>
                    {
                        // If there are any unmatched keyword parameters in `other`, they need to
                        // be checked against the keyword-only / keyword-variadic parameters that
                        // will be done after this loop.
                        break;
                    }
                    ParameterKind::PositionalOnly { default_type, .. }
                    | ParameterKind::PositionalOrKeyword { default_type, .. }
                    | ParameterKind::KeywordOnly { default_type, .. } => {
                        // For `source <: target` to be valid, if there are no more parameters in
                        // `target`, then the non-variadic parameters in `source` must have a default
                        // value.
                        if default_type.is_none() {
                            return self.never();
                        }
                    }
                    ParameterKind::Variadic { .. } | ParameterKind::KeywordVariadic { .. } => {
                        // Variadic parameters don't have any restrictions in this context, so
                        // we'll just continue to the next parameter set.
                    }
                },

                EitherOrBoth::Right(_) => {
                    // If there are more parameters in `target` than in `source`, then `source` is
                    // not a subtype of `target`.
                    return self.never();
                }

                EitherOrBoth::Both(source_param, target_param) => {
                    match (source_param.kind(), target_param.kind()) {
                        (
                            ParameterKind::PositionalOnly {
                                default_type: source_default,
                                ..
                            }
                            | ParameterKind::PositionalOrKeyword {
                                default_type: source_default,
                                ..
                            },
                            ParameterKind::PositionalOnly {
                                default_type: target_default,
                                ..
                            },
                        ) => {
                            if source_default.is_none() && target_default.is_some() {
                                return self.never();
                            }
                            if !check_types(
                                target_param.annotated_type(),
                                source_param.annotated_type(),
                                target_param.name(),
                                target_index,
                            ) {
                                return result;
                            }
                        }

                        (
                            ParameterKind::PositionalOrKeyword {
                                name: source_name,
                                default_type: source_default,
                            },
                            ParameterKind::PositionalOrKeyword {
                                name: target_name,
                                default_type: target_default,
                            },
                        ) => {
                            if source_name != target_name {
                                self.provide_context(|| ErrorContext::ParameterNameMismatch {
                                    source_name: source_name.clone(),
                                    target_name: target_name.clone(),
                                });
                                return self.never();
                            }
                            // The following checks are the same as positional-only parameters.
                            if source_default.is_none() && target_default.is_some() {
                                return self.never();
                            }
                            if !check_types(
                                target_param.annotated_type(),
                                source_param.annotated_type(),
                                target_param.name(),
                                target_index,
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
                                target_param.annotated_type(),
                                source_param.annotated_type(),
                                target_param.name(),
                                target_index,
                            ) {
                                return result;
                            }

                            if matches!(
                                target_param.kind(),
                                ParameterKind::PositionalOrKeyword { .. }
                            ) {
                                target_keywords.push(target_param);
                            }

                            // We've reached a variadic parameter in `source` which means there can
                            // be no more positional parameters after this in a valid AST. But, the
                            // current parameter in `target` is a positional-only which means there
                            // can be more positional parameters after this which could be either
                            // more positional-only parameters, standard parameters or a variadic
                            // parameter.
                            //
                            // So, any remaining positional parameters in `target` would need to be
                            // checked against the variadic parameter in `source`. This loop does
                            // that by only moving the `other` iterator forward.
                            while let Some(target_parameter) = parameters.peek_target() {
                                match target_parameter.kind() {
                                    ParameterKind::PositionalOrKeyword { .. } => {
                                        target_keywords.push(target_parameter);
                                    }
                                    ParameterKind::PositionalOnly { .. }
                                    | ParameterKind::Variadic { .. } => {}
                                    _ => {
                                        // Any other parameter kind cannot be checked against a
                                        // variadic parameter and is deferred to the next iteration.
                                        break;
                                    }
                                }
                                target_index += 1;
                                if !check_types(
                                    target_parameter.annotated_type(),
                                    source_param.annotated_type(),
                                    target_parameter.name(),
                                    target_index,
                                ) {
                                    return result;
                                }
                                parameters.next_target();
                            }
                        }

                        (ParameterKind::Variadic { .. }, ParameterKind::Variadic { .. }) => {
                            if !check_types(
                                target_param.annotated_type(),
                                source_param.annotated_type(),
                                target_param.name(),
                                target_index,
                            ) {
                                return result;
                            }
                        }

                        (
                            ParameterKind::PositionalOnly { name, .. },
                            ParameterKind::PositionalOrKeyword {
                                name: target_name, ..
                            },
                        ) => {
                            self.provide_context(|| {
                                ErrorContext::ParameterMustAcceptKeywordArguments {
                                    source_name: name.clone(),
                                    target_name: target_name.clone(),
                                }
                            });
                            return self.never();
                        }

                        (
                            ParameterKind::KeywordOnly { name, .. },
                            ParameterKind::PositionalOnly { .. }
                            | ParameterKind::PositionalOrKeyword { .. },
                        ) => {
                            self.provide_context(|| {
                                ErrorContext::ParameterMustAcceptPositionalArguments {
                                    name: name.clone(),
                                }
                            });
                            return self.never();
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

                        _ => return self.never(),
                    }
                    target_index += 1;
                }
            }
        }

        // At this point, the remaining parameters in `target` are keyword-only or keyword-variadic.
        // But, `source` could contain any unmatched positional parameters.
        let (source_params, target_params) = parameters.into_remaining();

        // Collect all the keyword-only parameters and the unmatched standard parameters.
        let mut source_keywords = FxHashMap::default();

        // Type of the variadic keyword parameter in `source`.
        //
        // This is an option representing the presence (and annotated type) of a keyword-variadic
        // parameter in `source`.
        let mut source_keyword_variadic: Option<Type<'db>> = None;

        for source_param in source_params {
            match source_param.kind() {
                ParameterKind::KeywordOnly { name, .. }
                | ParameterKind::PositionalOrKeyword { name, .. } => {
                    source_keywords.insert(name.as_str(), source_param);
                }
                ParameterKind::KeywordVariadic { .. } => {
                    source_keyword_variadic = Some(source_param.annotated_type());
                }
                ParameterKind::PositionalOnly { default_type, .. } => {
                    // These are the unmatched positional-only parameters in `source` from the
                    // previous loop. They cannot be matched against any parameter in `target` which
                    // only contains keyword-only and keyword-variadic parameters. However, if the
                    // parameter has a default, it's valid because callers don't need to provide it.
                    if default_type.is_none() {
                        return self.never();
                    }
                }
                ParameterKind::Variadic { .. } => {}
            }
        }

        for target_param in target_keywords.into_iter().chain(target_params) {
            match target_param.kind() {
                ParameterKind::KeywordOnly {
                    name: target_name,
                    default_type: target_default,
                }
                | ParameterKind::PositionalOrKeyword {
                    name: target_name,
                    default_type: target_default,
                } => {
                    if let Some(source_param) = source_keywords.remove(&**target_name) {
                        match source_param.kind() {
                            ParameterKind::PositionalOrKeyword {
                                default_type: source_default,
                                ..
                            }
                            | ParameterKind::KeywordOnly {
                                default_type: source_default,
                                ..
                            } => {
                                if source_default.is_none() && target_default.is_some() {
                                    return self.never();
                                }
                                if !check_types(
                                    target_param.annotated_type(),
                                    source_param.annotated_type(),
                                    target_param.name(),
                                    target_index,
                                ) {
                                    return result;
                                }
                            }
                            _ => unreachable!(
                                "`source_keywords` should only contain keyword-only or standard parameters"
                            ),
                        }
                    } else if let Some(source_keyword_variadic) = source_keyword_variadic {
                        if !check_types(
                            target_param.annotated_type(),
                            source_keyword_variadic,
                            target_param.name(),
                            target_index,
                        ) {
                            return result;
                        }
                    } else {
                        return self.never();
                    }
                }
                ParameterKind::KeywordVariadic { .. } => {
                    let Some(source_keyword_variadic) = source_keyword_variadic else {
                        // For a `source <: target` relationship, if `target` has a keyword variadic
                        // parameter, `source` must also have a keyword variadic parameter.
                        return self.never();
                    };
                    if !check_types(
                        target_param.annotated_type(),
                        source_keyword_variadic,
                        target_param.name(),
                        target_index,
                    ) {
                        return result;
                    }
                }
                _ => {
                    // This can only occur in case of a syntax error.
                    return self.never();
                }
            }
        }

        // If there are still unmatched keyword parameters from `source`, then they should be
        // optional otherwise the subtype relation is invalid.
        for (_, source_param) in source_keywords {
            if source_param.default_type().is_none() {
                return self.never();
            }
        }

        result
    }
}

/// The tail of a `Concatenate[T1, T2, Tn, tail]` form.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) enum ConcatenateTail<'db> {
    /// Represents the `Concatenate[T1, T2, Tn, ...]` form where the prefix parameters are followed
    /// by a gradual `*args: Any, **kwargs: Any`.
    Gradual,

    /// Represents the `Concatenate[T1, T2, Tn, P]` form where the prefix parameters are followed by
    /// a `ParamSpec` type variable.
    ParamSpec(BoundTypeVarInstance<'db>),
}

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

    /// Represents a parameter list containing a `ParamSpec` as the _only_ parameter.
    ///
    /// Note that this is distinct from a parameter list _containing_ a `ParamSpec` which is
    /// represented using the `Concatenate` variant.
    ParamSpec(BoundTypeVarInstance<'db>),

    /// Represents a parameter list containing positional-only parameters followed by either a
    /// gradual form (`...`) or a `ParamSpec`.
    ///
    /// This is used to represent the parameter list of a `Concatenate[T1, T2, Tn, ...]` and
    /// `Concatenate[T1, T2, Tn, P]` form.
    Concatenate(ConcatenateTail<'db>),
}

/// Represents a list of parameters in a function signature.
///
/// ## Representation
///
/// The way this is represented internally is a bit subtle given that both `value` and `kind` fields
/// need to follow certain invariants to correctly represent the different forms of parameter lists.
///
/// The `value` field should always contain the full list of parameters regardless of the `kind`
/// variant. For example, even if this represents a `Gradual` form, the `value` field should still
/// contain the `*args: Any` and `**kwargs: Any` parameter.
///
/// The `kind` field is used to indicate the specific form of the parameter list which can,
/// optionally, include additional information such as the bound `ParamSpec` type variable.
// TODO: Given how the current structure is laid out which needs to follow certain invariants
// between the `value` and `kind` field, it would be better to structure it such that these
// invariants are followed at the type level instead.
#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct Parameters<'db> {
    // TODO: use SmallVec here once invariance bug is fixed
    value: Vec<Parameter<'db>>,
    kind: ParametersKind<'db>,
}

impl<'db> Parameters<'db> {
    /// Create a new parameter list from an iterator of parameters.
    ///
    /// The kind of the parameter list is determined based on the provided parameters. Specifically,
    /// if the parameter list contains `*args` and `**kwargs`, then it checks their annotated types
    /// and the presence of other parameter kinds to determine if they represent a gradual form, a
    /// `ParamSpec`, or a `Concatenate` form.
    pub(crate) fn new(
        db: &'db dyn Db,
        parameters: impl IntoIterator<Item = Parameter<'db>>,
    ) -> Self {
        let value: Vec<Parameter<'db>> = parameters.into_iter().collect();
        let mut kind = ParametersKind::Standard;

        let variadic_param = value
            .iter()
            .find_position(|param| param.is_variadic())
            .map(|(index, param)| (index, param.annotated_type));
        let keyword_variadic_param = value
            .iter()
            .find_position(|param| param.is_keyword_variadic())
            .map(|(index, param)| (index, param.annotated_type));

        if let (
            Some((variadic_index, variadic_type)),
            Some((keyword_variadic_index, keyword_variadic_type)),
        ) = (variadic_param, keyword_variadic_param)
        {
            let prefix_params = value.get(..variadic_index).unwrap_or(&[]);
            let keyword_only_params = value
                .get(variadic_index + 1..keyword_variadic_index)
                .unwrap_or(&[]);

            match (variadic_type, keyword_variadic_type) {
                // > If the input signature in a function definition includes both a `*args` and
                // > `**kwargs` parameter and both are typed as Any (explicitly or implicitly
                // > because it has no annotation), a type checker should treat this as the
                // > equivalent of `...`. Any other parameters in the signature are unaffected and
                // > are retained as part of the signature.
                //
                // https://typing.python.org/en/latest/spec/callables.html#meaning-of-in-callable
                (Type::Dynamic(_), Type::Dynamic(_)) => {
                    if keyword_only_params.is_empty()
                        && !prefix_params.is_empty()
                        && prefix_params.iter().all(Parameter::is_positional_only)
                    {
                        kind = ParametersKind::Concatenate(ConcatenateTail::Gradual);
                    } else {
                        kind = ParametersKind::Gradual;
                    }
                }

                // > A function declared as
                // > `def inner(a: A, b: B, *args: P.args, **kwargs: P.kwargs) -> R`
                // > has type `Callable[Concatenate[A, B, P], R]`. Placing keyword-only parameters
                // > between the `*args` and `**kwargs` is forbidden.
                //
                // https://typing.python.org/en/latest/spec/generics.html#id5
                (Type::TypeVar(variadic_typevar), Type::TypeVar(keyword_variadic_typevar))
                    if keyword_only_params.is_empty() =>
                {
                    if let (Some(ParamSpecAttrKind::Args), Some(ParamSpecAttrKind::Kwargs)) = (
                        variadic_typevar.paramspec_attr(db),
                        keyword_variadic_typevar.paramspec_attr(db),
                    ) {
                        let typevar = variadic_typevar.without_paramspec_attr(db);
                        if typevar.is_same_typevar_as(
                            db,
                            keyword_variadic_typevar.without_paramspec_attr(db),
                        ) {
                            if prefix_params.is_empty() {
                                kind = ParametersKind::ParamSpec(typevar);
                            } else if prefix_params.iter().all(Parameter::is_positional) {
                                // TODO: Currently, we accept both positional-only and
                                // positional-or-keyword parameter but we should raise a warning to
                                // let users know that these parameters should be positional-only
                                kind = ParametersKind::Concatenate(ConcatenateTail::ParamSpec(
                                    typevar,
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        Parameters { value, kind }
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

    /// Returns `true` if the parameters represent a gradual form using `...` as the only parameter
    /// or a `Concatenate` form with `...` as the last argument.
    pub(crate) const fn is_gradual(&self) -> bool {
        matches!(
            self.kind,
            ParametersKind::Gradual | ParametersKind::Concatenate(ConcatenateTail::Gradual)
        )
    }

    pub(crate) const fn is_top(&self) -> bool {
        matches!(self.kind, ParametersKind::Top)
    }

    /// Returns the bound `ParamSpec` type variable if the parameter list is exactly `P`.
    ///
    /// For either `P` or `Concatenate[<prefix-params>, P]`, use [`as_paramspec_with_prefix`].
    ///
    /// [`as_paramspec_with_prefix`]: Self::as_paramspec_with_prefix
    pub(crate) const fn as_paramspec(&self) -> Option<BoundTypeVarInstance<'db>> {
        match self.kind {
            ParametersKind::ParamSpec(bound_typevar) => Some(bound_typevar),
            _ => None,
        }
    }

    /// Returns the prefix parameters and bound `ParamSpec` if this parameter list is either `P` or
    /// `Concatenate[<prefix-params>, P]`.
    ///
    /// For the narrower bare-`P` case, use [`as_paramspec`].
    ///
    /// [`as_paramspec`]: Self::as_paramspec
    pub(crate) fn as_paramspec_with_prefix<'a>(
        &'a self,
    ) -> Option<(&'a [Parameter<'db>], BoundTypeVarInstance<'db>)> {
        match self.kind {
            ParametersKind::ParamSpec(typevar) => Some((&[], typevar)),
            ParametersKind::Concatenate(ConcatenateTail::ParamSpec(typevar)) => {
                Some((&self.value[..self.value.len().saturating_sub(2)], typevar))
            }
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
    /// [`Any`]: DynamicType::Any
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

    /// Create a parameter list representing a `Concatenate` form with the given prefix parameters
    /// and the tail (either gradual or a `ParamSpec`).
    ///
    /// Internally, this is represented as either:
    /// - `(<prefix_params>, /, *args: Any, **kwargs: Any)` for the gradual form, or
    /// - `(<prefix_params>, /, *args: P.args, **kwargs: P.kwargs)` for the `ParamSpec` form.
    pub(crate) fn concatenate(
        db: &'db dyn Db,
        mut prefix_params: Vec<Parameter<'db>>,
        concatenate_tail: ConcatenateTail<'db>,
    ) -> Self {
        let (args_type, kwargs_type) = match concatenate_tail {
            ConcatenateTail::Gradual => (Type::any(), Type::any()),
            ConcatenateTail::ParamSpec(typevar) => (
                Type::TypeVar(typevar.with_paramspec_attr(db, ParamSpecAttrKind::Args)),
                Type::TypeVar(typevar.with_paramspec_attr(db, ParamSpecAttrKind::Kwargs)),
            ),
        };
        prefix_params.extend([
            Parameter::variadic(Name::new_static("args")).with_annotated_type(args_type),
            Parameter::keyword_variadic(Name::new_static("kwargs"))
                .with_annotated_type(kwargs_type),
        ]);
        Self {
            value: prefix_params,
            kind: ParametersKind::Concatenate(concatenate_tail),
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
                // Use the same approach as function_signature_expression_type to avoid cycles.
                // Defaults are always deferred (see infer_function_definition), so we can go
                // directly to infer_deferred_types without first checking infer_definition_types.
                infer_deferred_types(db, definition)
                    .expression_type(default)
                    .replace_parameter_defaults(db)
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
            && matches!(
                self.kind,
                ParametersKind::Gradual | ParametersKind::Concatenate(ConcatenateTail::Gradual)
            )
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

    /// Expands adjacent `P.args`/`P.kwargs` placeholders into their mapped parameters.
    pub(crate) fn expand_paramspec_variadics(&self, db: &'db dyn Db) -> Self {
        let mut variadic_index = None;
        let mut paramspec_callable = None;

        for (index, parameter) in self.iter().enumerate() {
            if !parameter.is_variadic() {
                continue;
            }

            let Type::Callable(callable) = parameter.annotated_type() else {
                continue;
            };
            if callable.kind(db) != CallableTypeKind::ParamSpecValue {
                continue;
            }

            variadic_index = Some(index);
            paramspec_callable = Some(callable);
            break;
        }

        let Some(variadic_index) = variadic_index else {
            return self.clone();
        };
        let Some(paramspec_callable) = paramspec_callable else {
            return self.clone();
        };

        let Some(keyword_variadic) = self.get(variadic_index + 1) else {
            return self.clone();
        };
        if !keyword_variadic.is_keyword_variadic() {
            return self.clone();
        }

        let Type::Callable(keyword_callable) = keyword_variadic.annotated_type() else {
            return self.clone();
        };
        if keyword_callable.kind(db) != CallableTypeKind::ParamSpecValue
            || keyword_callable != paramspec_callable
        {
            return self.clone();
        }

        let [mapped_signature] = paramspec_callable.signatures(db).overloads.as_slice() else {
            return self.clone();
        };

        let mut expanded = Vec::with_capacity(self.len());
        expanded.extend_from_slice(&self.value[..variadic_index]);
        expanded.extend_from_slice(mapped_signature.parameters().as_slice());
        expanded.extend_from_slice(&self.value[variadic_index + 2..]);
        Parameters::new(db, expanded)
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
    /// Annotated type of the parameter. If no annotation was provided, this is `Unknown`.
    annotated_type: Type<'db>,

    /// Does the type of this parameter come from an explicit annotation, or was it inferred from
    /// the context, like `Unknown` for any normal un-annotated parameter, `Self` for the `self`
    /// parameter of instance method, or `type[Self]` for `cls` parameter of classmethods. This
    /// field is only used to decide whether to display the annotated type; it has no effect on the
    /// type semantics of the parameter.
    pub(crate) inferred_annotation: bool,

    /// Variadic parameters can have starred annotations, e.g.
    /// - `*args: *Ts`
    /// - `*args: *tuple[int, ...]`
    /// - `*args: *tuple[int, *tuple[str, ...], bytes]`
    ///
    /// The `*` prior to the type gives the annotation a different meaning,
    /// so this must be propagated upwards.
    has_starred_annotation: bool,

    kind: ParameterKind<'db>,
    pub(crate) form: ParameterForm,
}

impl<'db> Parameter<'db> {
    pub(crate) fn positional_only(name: Option<Name>) -> Self {
        Self {
            annotated_type: Type::unknown(),
            inferred_annotation: true,
            has_starred_annotation: false,
            kind: ParameterKind::PositionalOnly {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn positional_or_keyword(name: Name) -> Self {
        Self {
            annotated_type: Type::unknown(),
            inferred_annotation: true,
            has_starred_annotation: false,
            kind: ParameterKind::PositionalOrKeyword {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn variadic(name: Name) -> Self {
        Self {
            annotated_type: Type::unknown(),
            inferred_annotation: true,
            has_starred_annotation: false,
            kind: ParameterKind::Variadic { name },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn keyword_only(name: Name) -> Self {
        Self {
            annotated_type: Type::unknown(),
            inferred_annotation: true,
            has_starred_annotation: false,
            kind: ParameterKind::KeywordOnly {
                name,
                default_type: None,
            },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn keyword_variadic(name: Name) -> Self {
        Self {
            annotated_type: Type::unknown(),
            inferred_annotation: true,
            has_starred_annotation: false,
            kind: ParameterKind::KeywordVariadic { name },
            form: ParameterForm::Value,
        }
    }

    /// Set the annotated type for this parameter. This also marks the annotation as explicit
    /// (not inferred), so it will be displayed.
    pub(crate) fn with_annotated_type(mut self, annotated_type: Type<'db>) -> Self {
        self.annotated_type = annotated_type;
        self.inferred_annotation = false;
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

    pub(crate) fn with_optional_default_type(self, default: Option<Type<'db>>) -> Self {
        if let Some(default) = default {
            self.with_default_type(default)
        } else {
            self
        }
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
            annotated_type: self.annotated_type.apply_type_mapping_impl(
                db,
                type_mapping,
                tcx,
                visitor,
            ),
            kind: self
                .kind
                .apply_type_mapping_impl(db, type_mapping, tcx, visitor),
            inferred_annotation: self.inferred_annotation,
            has_starred_annotation: self.has_starred_annotation,
            form: self.form,
        }
    }

    fn cycle_normalized(&self, db: &'db dyn Db, previous: &Self, cycle: &salsa::Cycle) -> Self {
        let annotated_type =
            self.annotated_type
                .cycle_normalized(db, previous.annotated_type, cycle);

        let kind = self.kind.cycle_normalized(db, &previous.kind, cycle);

        Self {
            annotated_type,
            inferred_annotation: self.inferred_annotation,
            has_starred_annotation: self.has_starred_annotation,
            kind,
            form: self.form,
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
            has_starred_annotation,
            inferred_annotation,
            kind,
            form,
        } = self;

        let annotated_type = if nested {
            annotated_type.recursive_type_normalized_impl(db, div, true)?
        } else {
            annotated_type
                .recursive_type_normalized_impl(db, div, true)
                .unwrap_or(div)
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
            has_starred_annotation: *has_starred_annotation,
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
        let (annotated_type, inferred_annotation, has_starred_annotation) =
            if let Some(annotation) = parameter.annotation() {
                (
                    function_signature_expression_type(db, definition, annotation),
                    false,
                    annotation.is_starred_expr(),
                )
            } else {
                (Type::unknown(), true, false)
            };
        Self {
            annotated_type,
            kind,
            has_starred_annotation,
            form: ParameterForm::Value,
            inferred_annotation,
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

    /// Returns the name of this parameter if it is a keyword-only or standard parameter.
    pub(crate) fn keyword_name(&self) -> Option<&Name> {
        match &self.kind {
            ParameterKind::PositionalOrKeyword { name, .. }
            | ParameterKind::KeywordOnly { name, .. } => Some(name),
            _ => None,
        }
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

    /// Annotated type of the parameter. If no annotation was provided, this is `Unknown`.
    pub(crate) fn annotated_type(&self) -> Type<'db> {
        self.annotated_type
    }

    /// Return `true` if this parameter has a starred annotation,
    /// e.g. `*args: *Ts` or `*args: *tuple[int, *tuple[str, ...], bytes]`
    pub(crate) fn has_starred_annotation(&self) -> bool {
        self.has_starred_annotation
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

    /// Rewrites a positional-or-keyword parameter as keyword-only while preserving its metadata.
    pub(crate) fn positional_or_keyword_to_keyword_only(&self) -> Self {
        let mut result = self.clone();
        if let ParameterKind::PositionalOrKeyword { name, default_type } = &self.kind {
            result.kind = ParameterKind::KeywordOnly {
                name: name.clone(),
                default_type: *default_type,
            };
        }
        result
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
    #[expect(clippy::ref_option)]
    fn cycle_normalized_default(
        db: &'db dyn Db,
        current: &Option<Type<'db>>,
        previous: &Option<Type<'db>>,
        cycle: &salsa::Cycle,
    ) -> Option<Type<'db>> {
        match (current, previous) {
            (Some(curr), Some(prev)) => Some(curr.cycle_normalized(db, *prev, cycle)),
            (Some(curr), None) => Some(curr.recursive_type_normalized(db, cycle)),
            (None, _) => *current,
        }
    }

    fn cycle_normalized(&self, db: &'db dyn Db, previous: &Self, cycle: &salsa::Cycle) -> Self {
        match (self, previous) {
            (
                ParameterKind::PositionalOnly { name, default_type },
                ParameterKind::PositionalOnly {
                    default_type: prev_default,
                    ..
                },
            ) => ParameterKind::PositionalOnly {
                name: name.clone(),
                default_type: Self::cycle_normalized_default(db, default_type, prev_default, cycle),
            },
            (
                ParameterKind::PositionalOrKeyword { name, default_type },
                ParameterKind::PositionalOrKeyword {
                    default_type: prev_default,
                    ..
                },
            ) => ParameterKind::PositionalOrKeyword {
                name: name.clone(),
                default_type: Self::cycle_normalized_default(db, default_type, prev_default, cycle),
            },
            (
                ParameterKind::KeywordOnly { name, default_type },
                ParameterKind::KeywordOnly {
                    default_type: prev_default,
                    ..
                },
            ) => ParameterKind::KeywordOnly {
                name: name.clone(),
                default_type: Self::cycle_normalized_default(db, default_type, prev_default, cycle),
            },
            // Variadic / KeywordVariadic have no types to normalize.
            // Also, if the current `ParameterKind` is different from `previous`, it means that `previous` is the cycle initial value,
            // and the current value should take precedence.
            _ => self.clone(),
        }
    }

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
    use crate::types::{FunctionType, KnownClass, LiteralValueType};
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
            .last_definition;

        let sig = func.signature(&db);

        assert!(sig.return_ty.is_unknown());
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
            .last_definition;

        let sig = func.signature(&db);

        assert_eq!(sig.return_ty.display(&db).to_string(), "bytes");
        assert_params(
            &sig,
            &[
                Parameter::positional_only(Some(Name::new_static("a"))),
                Parameter::positional_only(Some(Name::new_static("b")))
                    .with_annotated_type(KnownClass::Int.to_instance(&db)),
                Parameter::positional_only(Some(Name::new_static("c")))
                    .with_default_type(Type::int_literal(1)),
                Parameter::positional_only(Some(Name::new_static("d")))
                    .with_annotated_type(KnownClass::Int.to_instance(&db))
                    .with_default_type(Type::int_literal(2)),
                Parameter::positional_or_keyword(Name::new_static("e"))
                    .with_default_type(Type::int_literal(3)),
                Parameter::positional_or_keyword(Name::new_static("f"))
                    .with_annotated_type(LiteralValueType::unpromotable(4).into())
                    .with_default_type(LiteralValueType::unpromotable(4).into()),
                Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::object()),
                Parameter::keyword_only(Name::new_static("g"))
                    .with_default_type(Type::int_literal(5)),
                Parameter::keyword_only(Name::new_static("h"))
                    .with_annotated_type(LiteralValueType::unpromotable(6).into())
                    .with_default_type(LiteralValueType::unpromotable(6).into()),
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
            .last_definition;

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
        assert_eq!(annotated_type.display(&db).to_string(), "A");
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
            .last_definition;

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
        assert_eq!(annotated_type.display(&db).to_string(), "A | B");
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
            .last_definition;

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
        assert_eq!(a_annotated_ty.display(&db).to_string(), "A");
        assert_eq!(b_annotated_ty.display(&db).to_string(), "T@f");
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
            .last_definition;

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
        assert_eq!(a_annotated_ty.display(&db).to_string(), "A | B");
        assert_eq!(b_annotated_ty.display(&db).to_string(), "T@f");
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

        let overload = func.literal(&db).last_definition;
        let expected_sig = overload.signature(&db);

        // With no decorators, internal and external signature are the same
        assert_eq!(
            func.signature(&db),
            &CallableSignature::single(expected_sig)
        );
    }
}
