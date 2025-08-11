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

use std::{collections::HashMap, slice::Iter};

use itertools::EitherOrBoth;
use smallvec::{SmallVec, smallvec_inline};

use super::{DynamicType, Type, TypeTransformer, TypeVarVariance, definition_expression_type};
use crate::semantic_index::definition::Definition;
use crate::types::generics::{GenericContext, walk_generic_context};
use crate::types::{KnownClass, TypeMapping, TypeRelation, TypeVarInstance, todo_type};
use crate::{Db, FxOrderSet};
use ruff_python_ast::{self as ast, name::Name};

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

    pub(super) fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self::from_overloads(
            self.overloads
                .iter()
                .map(|signature| signature.materialize(db, variance)),
        )
    }

    pub(crate) fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &mut TypeTransformer<'db>,
    ) -> Self {
        Self::from_overloads(
            self.overloads
                .iter()
                .map(|signature| signature.normalized_impl(db, visitor)),
        )
    }

    pub(crate) fn apply_type_mapping<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        Self::from_overloads(
            self.overloads
                .iter()
                .map(|signature| signature.apply_type_mapping(db, type_mapping)),
        )
    }

    pub(crate) fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        for signature in &self.overloads {
            signature.find_legacy_typevars(db, typevars);
        }
    }

    pub(crate) fn bind_self(&self) -> Self {
        Self {
            overloads: self.overloads.iter().map(Signature::bind_self).collect(),
        }
    }

    pub(crate) fn has_relation_to(
        &self,
        db: &'db dyn Db,
        other: &Self,
        relation: TypeRelation,
    ) -> bool {
        match relation {
            TypeRelation::Subtyping => self.is_subtype_of(db, other),
            TypeRelation::Assignability => self.is_assignable_to(db, other),
        }
    }

    /// Check whether this callable type is a subtype of another callable type.
    ///
    /// See [`Type::is_subtype_of`] for more details.
    pub(crate) fn is_subtype_of(&self, db: &'db dyn Db, other: &Self) -> bool {
        Self::has_relation_to_impl(
            db,
            &self.overloads,
            &other.overloads,
            TypeRelation::Subtyping,
        )
    }

    /// Check whether this callable type is assignable to another callable type.
    ///
    /// See [`Type::is_assignable_to`] for more details.
    pub(crate) fn is_assignable_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        Self::has_relation_to_impl(
            db,
            &self.overloads,
            &other.overloads,
            TypeRelation::Assignability,
        )
    }

    /// Implementation of subtyping and assignability between two, possible overloaded, callable
    /// types.
    fn has_relation_to_impl(
        db: &'db dyn Db,
        self_signatures: &[Signature<'db>],
        other_signatures: &[Signature<'db>],
        relation: TypeRelation,
    ) -> bool {
        match (self_signatures, other_signatures) {
            ([self_signature], [other_signature]) => {
                // Base case: both callable types contain a single signature.
                self_signature.has_relation_to(db, other_signature, relation)
            }

            // `self` is possibly overloaded while `other` is definitely not overloaded.
            (_, [_]) => self_signatures.iter().any(|self_signature| {
                Self::has_relation_to_impl(
                    db,
                    std::slice::from_ref(self_signature),
                    other_signatures,
                    relation,
                )
            }),

            // `self` is definitely not overloaded while `other` is possibly overloaded.
            ([_], _) => other_signatures.iter().all(|other_signature| {
                Self::has_relation_to_impl(
                    db,
                    self_signatures,
                    std::slice::from_ref(other_signature),
                    relation,
                )
            }),

            // `self` is definitely overloaded while `other` is possibly overloaded.
            (_, _) => other_signatures.iter().all(|other_signature| {
                Self::has_relation_to_impl(
                    db,
                    self_signatures,
                    std::slice::from_ref(other_signature),
                    relation,
                )
            }),
        }
    }

    /// Check whether this callable type is equivalent to another callable type.
    ///
    /// See [`Type::is_equivalent_to`] for more details.
    pub(crate) fn is_equivalent_to(&self, db: &'db dyn Db, other: &Self) -> bool {
        match (self.overloads.as_slice(), other.overloads.as_slice()) {
            ([self_signature], [other_signature]) => {
                // Common case: both callable types contain a single signature, use the custom
                // equivalence check instead of delegating it to the subtype check.
                self_signature.is_equivalent_to(db, other_signature)
            }
            (_, _) => {
                if self == other {
                    return true;
                }
                self.is_subtype_of(db, other) && other.is_subtype_of(db, self)
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

/// The signature of one of the overloads of a callable.
#[derive(Clone, Debug, salsa::Update, get_size2::GetSize)]
pub struct Signature<'db> {
    /// The generic context for this overload, if it is generic.
    pub(crate) generic_context: Option<GenericContext<'db>>,

    /// The inherited generic context, if this function is a class method being used to infer the
    /// specialization of its generic class. If the method is itself generic, this is in addition
    /// to its own generic context.
    pub(crate) inherited_generic_context: Option<GenericContext<'db>>,

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
    visitor: &mut V,
) {
    if let Some(generic_context) = &signature.generic_context {
        walk_generic_context(db, *generic_context, visitor);
    }
    if let Some(inherited_generic_context) = &signature.inherited_generic_context {
        walk_generic_context(db, *inherited_generic_context, visitor);
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
            inherited_generic_context: None,
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
            inherited_generic_context: None,
            definition: None,
            parameters,
            return_ty,
        }
    }

    /// Return a signature for a dynamic callable
    pub(crate) fn dynamic(signature_type: Type<'db>) -> Self {
        Signature {
            generic_context: None,
            inherited_generic_context: None,
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
            inherited_generic_context: None,
            definition: None,
            parameters: Parameters::todo(),
            return_ty: Some(signature_type),
        }
    }

    /// Return a typed signature from a function definition.
    pub(super) fn from_function(
        db: &'db dyn Db,
        generic_context: Option<GenericContext<'db>>,
        inherited_generic_context: Option<GenericContext<'db>>,
        definition: Definition<'db>,
        function_node: &ast::StmtFunctionDef,
        is_generator: bool,
    ) -> Self {
        let parameters =
            Parameters::from_parameters(db, definition, function_node.parameters.as_ref());
        let return_ty = function_node.returns.as_ref().map(|returns| {
            let plain_return_ty = definition_expression_type(db, definition, returns.as_ref());

            if function_node.is_async && !is_generator {
                KnownClass::CoroutineType
                    .to_specialized_instance(db, [Type::any(), Type::any(), plain_return_ty])
            } else {
                plain_return_ty
            }
        });
        let legacy_generic_context =
            GenericContext::from_function_params(db, definition, &parameters, return_ty);

        if generic_context.is_some() && legacy_generic_context.is_some() {
            // TODO: Raise a diagnostic!
        }

        Self {
            generic_context: generic_context.or(legacy_generic_context),
            inherited_generic_context,
            definition: Some(definition),
            parameters,
            return_ty,
        }
    }

    /// Returns the signature which accepts any parameters and returns an `Unknown` type.
    pub(crate) fn unknown() -> Self {
        Self::new(Parameters::unknown(), Some(Type::unknown()))
    }

    /// Return the "bottom" signature, subtype of all other fully-static signatures.
    pub(crate) fn bottom(db: &'db dyn Db) -> Self {
        Self::new(Parameters::object(db), Some(Type::Never))
    }

    pub(crate) fn with_inherited_generic_context(
        mut self,
        inherited_generic_context: Option<GenericContext<'db>>,
    ) -> Self {
        self.inherited_generic_context = inherited_generic_context;
        self
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self {
            generic_context: self.generic_context,
            inherited_generic_context: self.inherited_generic_context,
            definition: self.definition,
            // Parameters are at contravariant position, so the variance is flipped.
            parameters: self.parameters.materialize(db, variance.flip()),
            return_ty: Some(
                self.return_ty
                    .unwrap_or(Type::unknown())
                    .materialize(db, variance),
            ),
        }
    }

    pub(crate) fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &mut TypeTransformer<'db>,
    ) -> Self {
        Self {
            generic_context: self
                .generic_context
                .map(|ctx| ctx.normalized_impl(db, visitor)),
            inherited_generic_context: self
                .inherited_generic_context
                .map(|ctx| ctx.normalized_impl(db, visitor)),
            // Discard the definition when normalizing, so that two equivalent signatures
            // with different `Definition`s share the same Salsa ID when normalized
            definition: None,
            parameters: self
                .parameters
                .iter()
                .map(|param| param.normalized_impl(db, visitor))
                .collect(),
            return_ty: self
                .return_ty
                .map(|return_ty| return_ty.normalized_impl(db, visitor)),
        }
    }

    pub(crate) fn apply_type_mapping<'a>(
        &self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        Self {
            generic_context: self.generic_context,
            inherited_generic_context: self.inherited_generic_context,
            definition: self.definition,
            parameters: self.parameters.apply_type_mapping(db, type_mapping),
            return_ty: self
                .return_ty
                .map(|ty| ty.apply_type_mapping(db, type_mapping)),
        }
    }

    pub(crate) fn find_legacy_typevars(
        &self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        for param in &self.parameters {
            if let Some(ty) = param.annotated_type() {
                ty.find_legacy_typevars(db, typevars);
            }
            if let Some(ty) = param.default_type() {
                ty.find_legacy_typevars(db, typevars);
            }
        }
        if let Some(ty) = self.return_ty {
            ty.find_legacy_typevars(db, typevars);
        }
    }

    /// Return the parameters in this signature.
    pub(crate) fn parameters(&self) -> &Parameters<'db> {
        &self.parameters
    }

    /// Return the definition associated with this signature, if any.
    pub(crate) fn definition(&self) -> Option<Definition<'db>> {
        self.definition
    }

    pub(crate) fn bind_self(&self) -> Self {
        Self {
            generic_context: self.generic_context,
            inherited_generic_context: self.inherited_generic_context,
            definition: self.definition,
            parameters: Parameters::new(self.parameters().iter().skip(1).cloned()),
            return_ty: self.return_ty,
        }
    }

    /// Return `true` if `self` has exactly the same set of possible static materializations as
    /// `other` (if `self` represents the same set of possible sets of possible runtime objects as
    /// `other`).
    pub(crate) fn is_equivalent_to(&self, db: &'db dyn Db, other: &Signature<'db>) -> bool {
        let check_types = |self_type: Option<Type<'db>>, other_type: Option<Type<'db>>| {
            self_type
                .unwrap_or(Type::unknown())
                .is_equivalent_to(db, other_type.unwrap_or(Type::unknown()))
        };

        if self.parameters.is_gradual() != other.parameters.is_gradual() {
            return false;
        }

        if self.parameters.len() != other.parameters.len() {
            return false;
        }

        if !check_types(self.return_ty, other.return_ty) {
            return false;
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

                _ => return false,
            }

            if !check_types(
                self_parameter.annotated_type(),
                other_parameter.annotated_type(),
            ) {
                return false;
            }
        }

        true
    }

    /// Implementation of subtyping and assignability for signature.
    fn has_relation_to(
        &self,
        db: &'db dyn Db,
        other: &Signature<'db>,
        relation: TypeRelation,
    ) -> bool {
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

        let check_types = |type1: Option<Type<'db>>, type2: Option<Type<'db>>| {
            type1.unwrap_or(Type::unknown()).has_relation_to(
                db,
                type2.unwrap_or(Type::unknown()),
                relation,
            )
        };

        // Return types are covariant.
        if !check_types(self.return_ty, other.return_ty) {
            return false;
        }

        // A gradual parameter list is a supertype of the "bottom" parameter list (*args: object,
        // **kwargs: object).
        if other.parameters.is_gradual()
            && self
                .parameters
                .variadic()
                .is_some_and(|(_, param)| param.annotated_type().is_some_and(|ty| ty.is_object(db)))
            && self
                .parameters
                .keyword_variadic()
                .is_some_and(|(_, param)| param.annotated_type().is_some_and(|ty| ty.is_object(db)))
        {
            return true;
        }

        // If either of the parameter lists is gradual (`...`), then it is assignable to and from
        // any other parameter list, but not a subtype or supertype of any other parameter list.
        if self.parameters.is_gradual() || other.parameters.is_gradual() {
            return relation.is_assignability();
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
                return true;
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
                            return false;
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
                    return false;
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
                                return false;
                            }
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return false;
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
                                return false;
                            }
                            // The following checks are the same as positional-only parameters.
                            if self_default.is_none() && other_default.is_some() {
                                return false;
                            }
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return false;
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
                                return false;
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
                                    return false;
                                }
                                parameters.next_other();
                            }
                        }

                        (ParameterKind::Variadic { .. }, ParameterKind::Variadic { .. }) => {
                            if !check_types(
                                other_parameter.annotated_type(),
                                self_parameter.annotated_type(),
                            ) {
                                return false;
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

                        _ => return false,
                    }
                }
            }
        }

        // At this point, the remaining parameters in `other` are keyword-only or keyword variadic.
        // But, `self` could contain any unmatched positional parameters.
        let (self_parameters, other_parameters) = parameters.into_remaining();

        // Collect all the keyword-only parameters and the unmatched standard parameters.
        let mut self_keywords = HashMap::new();

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
                    self_keywords.insert(name.clone(), self_parameter);
                }
                ParameterKind::KeywordVariadic { .. } => {
                    self_keyword_variadic = Some(self_parameter.annotated_type());
                }
                ParameterKind::PositionalOnly { .. } => {
                    // These are the unmatched positional-only parameters in `self` from the
                    // previous loop. They cannot be matched against any parameter in `other` which
                    // only contains keyword-only and keyword-variadic parameters so the subtype
                    // relation is invalid.
                    return false;
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
                    if let Some(self_parameter) = self_keywords.remove(other_name) {
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
                                    return false;
                                }
                                if !check_types(
                                    other_parameter.annotated_type(),
                                    self_parameter.annotated_type(),
                                ) {
                                    return false;
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
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                ParameterKind::KeywordVariadic { .. } => {
                    let Some(self_keyword_variadic_type) = self_keyword_variadic else {
                        // For a `self <: other` relationship, if `other` has a keyword variadic
                        // parameter, `self` must also have a keyword variadic parameter.
                        return false;
                    };
                    if !check_types(other_parameter.annotated_type(), self_keyword_variadic_type) {
                        return false;
                    }
                }
                _ => {
                    // This can only occur in case of a syntax error.
                    return false;
                }
            }
        }

        // If there are still unmatched keyword parameters from `self`, then they should be
        // optional otherwise the subtype relation is invalid.
        for (_, self_parameter) in self_keywords {
            if self_parameter.default_type().is_none() {
                return false;
            }
        }

        true
    }

    /// Create a new signature with the given definition.
    pub(crate) fn with_definition(self, definition: Option<Definition<'db>>) -> Self {
        Self { definition, ..self }
    }
}

// Manual implementations of PartialEq, Eq, and Hash that exclude the definition field
// since the definition is not relevant for type equality/equivalence
impl PartialEq for Signature<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.generic_context == other.generic_context
            && self.inherited_generic_context == other.inherited_generic_context
            && self.parameters == other.parameters
            && self.return_ty == other.return_ty
    }
}

impl Eq for Signature<'_> {}

impl std::hash::Hash for Signature<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.generic_context.hash(state);
        self.inherited_generic_context.hash(state);
        self.parameters.hash(state);
        self.return_ty.hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, salsa::Update, get_size2::GetSize)]
pub(crate) struct Parameters<'db> {
    // TODO: use SmallVec here once invariance bug is fixed
    value: Vec<Parameter<'db>>,

    /// Whether this parameter list represents a gradual form using `...` as the only parameter.
    ///
    /// If this is `true`, the `value` will still contain the variadic and keyword-variadic
    /// parameters.
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
    /// TODO: the spec also allows signatures like `Concatenate[int, ...]`, which have some number
    /// of required positional parameters followed by a gradual form. Our representation will need
    /// some adjustments to represent that.
    ///
    ///   [the typing specification]: https://typing.python.org/en/latest/spec/callables.html#meaning-of-in-callable
    is_gradual: bool,
}

impl<'db> Parameters<'db> {
    pub(crate) fn new(parameters: impl IntoIterator<Item = Parameter<'db>>) -> Self {
        let value: Vec<Parameter<'db>> = parameters.into_iter().collect();
        let is_gradual = value.len() == 2
            && value
                .iter()
                .any(|p| p.is_variadic() && p.annotated_type().is_none_or(|ty| ty.is_dynamic()))
            && value.iter().any(|p| {
                p.is_keyword_variadic() && p.annotated_type().is_none_or(|ty| ty.is_dynamic())
            });
        Self { value, is_gradual }
    }

    /// Create an empty parameter list.
    pub(crate) fn empty() -> Self {
        Self {
            value: Vec::new(),
            is_gradual: false,
        }
    }

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        if self.is_gradual {
            Parameters::object(db)
        } else {
            Parameters::new(
                self.iter()
                    .map(|parameter| parameter.materialize(db, variance)),
            )
        }
    }

    pub(crate) fn as_slice(&self) -> &[Parameter<'db>] {
        self.value.as_slice()
    }

    pub(crate) const fn is_gradual(&self) -> bool {
        self.is_gradual
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
            is_gradual: true,
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
            is_gradual: true,
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
            is_gradual: true,
        }
    }

    /// Return parameters that represents `(*args: object, **kwargs: object)`.
    pub(crate) fn object(db: &'db dyn Db) -> Self {
        Self {
            value: vec![
                Parameter::variadic(Name::new_static("args")).with_annotated_type(Type::object(db)),
                Parameter::keyword_variadic(Name::new_static("kwargs"))
                    .with_annotated_type(Type::object(db)),
            ],
            is_gradual: false,
        }
    }

    fn from_parameters(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameters: &ast::Parameters,
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
            param
                .default()
                .map(|default| definition_expression_type(db, definition, default))
        };
        let positional_only = posonlyargs.iter().map(|arg| {
            Parameter::from_node_and_kind(
                db,
                definition,
                &arg.parameter,
                ParameterKind::PositionalOnly {
                    name: Some(arg.parameter.name.id.clone()),
                    default_type: default_type(arg),
                },
            )
        });
        let positional_or_keyword = args.iter().map(|arg| {
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
            positional_only
                .chain(positional_or_keyword)
                .chain(variadic)
                .chain(keyword_only)
                .chain(keywords),
        )
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        Self {
            value: self
                .value
                .iter()
                .map(|param| param.apply_type_mapping(db, type_mapping))
                .collect(),
            is_gradual: self.is_gradual,
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

impl<'db> FromIterator<Parameter<'db>> for Parameters<'db> {
    fn from_iter<T: IntoIterator<Item = Parameter<'db>>>(iter: T) -> Self {
        Self::new(iter)
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

    kind: ParameterKind<'db>,
    pub(crate) form: ParameterForm,
}

impl<'db> Parameter<'db> {
    pub(crate) fn positional_only(name: Option<Name>) -> Self {
        Self {
            annotated_type: None,
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
            kind: ParameterKind::Variadic { name },
            form: ParameterForm::Value,
        }
    }

    pub(crate) fn keyword_only(name: Name) -> Self {
        Self {
            annotated_type: None,
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

    fn materialize(&self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        Self {
            annotated_type: Some(
                self.annotated_type
                    .unwrap_or(Type::unknown())
                    .materialize(db, variance),
            ),
            kind: self.kind.clone(),
            form: self.form,
        }
    }

    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        Self {
            annotated_type: self
                .annotated_type
                .map(|ty| ty.apply_type_mapping(db, type_mapping)),
            kind: self.kind.apply_type_mapping(db, type_mapping),
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
        visitor: &mut TypeTransformer<'db>,
    ) -> Self {
        let Parameter {
            annotated_type,
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
            kind,
            form: *form,
        }
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
                .map(|annotation| definition_expression_type(db, definition, annotation)),
            kind,
            form: ParameterForm::Value,
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
pub(crate) enum ParameterKind<'db> {
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
    fn apply_type_mapping<'a>(&self, db: &'db dyn Db, type_mapping: &TypeMapping<'a, 'db>) -> Self {
        match self {
            Self::PositionalOnly { default_type, name } => Self::PositionalOnly {
                default_type: default_type
                    .as_ref()
                    .map(|ty| ty.apply_type_mapping(db, type_mapping)),
                name: name.clone(),
            },
            Self::PositionalOrKeyword { default_type, name } => Self::PositionalOrKeyword {
                default_type: default_type
                    .as_ref()
                    .map(|ty| ty.apply_type_mapping(db, type_mapping)),
                name: name.clone(),
            },
            Self::KeywordOnly { default_type, name } => Self::KeywordOnly {
                default_type: default_type
                    .as_ref()
                    .map(|ty| ty.apply_type_mapping(db, type_mapping)),
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

        let sig = func.signature(&db, None);

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

        let sig = func.signature(&db, None);

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
                Parameter::variadic(Name::new_static("args"))
                    .with_annotated_type(Type::object(&db)),
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

        let sig = func.signature(&db, None);

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

        let sig = func.signature(&db, None);

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

        let sig = func.signature(&db, None);

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
        // TODO resolution should not be deferred; we should see A, not A | B
        assert_eq!(
            a_annotated_ty.unwrap().display(&db).to_string(),
            "Unknown | A | B"
        );
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

        let sig = func.signature(&db, None);

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
        let expected_sig = overload.signature(&db, None);

        // With no decorators, internal and external signature are the same
        assert_eq!(
            func.signature(&db),
            &CallableSignature::single(expected_sig)
        );
    }
}
