//! Source signature construction with explicit semantic dependencies and storage admission.

use std::convert::Infallible;
use std::num::NonZeroU32;

use ruff_python_ast as ast;
use salsa::execution_probe::FieldRequest;
use ty_python_core::definition::Definition;
use ty_python_core::{ProgramFile, SemanticIndex, semantic_index};

use super::{
    ConcatenateTail, Parameter, ParameterAnnotationKind, ParameterDefault, ParameterKind,
    Parameters, ParametersData, ParametersKind, ReturnCallableTypeVarScope, Signature,
    SignatureExtras, TypeExpressionFlags, function_signature_expression_type,
    function_signature_type_expression_flags,
};
use crate::Db;
use crate::types::generics::GenericContext;
use crate::types::typed_dict::extract_unpacked_typed_dict_keys_from_kwargs_annotation;
use crate::types::{BoundTypeVarInstance, ParamSpecAttrKind, Type};

#[derive(Clone, Copy)]
pub(in crate::types) struct ParametersStorageQuote {
    pub work: usize,
    pub bytes: usize,
}

/// Quote the parameter array and its shared owner, including disposal of every parameter.
pub(in crate::types) fn parameters_storage_quote(count: usize) -> Option<ParametersStorageQuote> {
    Some(ParametersStorageQuote {
        work: count.checked_mul(8)?.checked_add(4)?,
        bytes: count
            .checked_mul(size_of::<Parameter<'_>>())?
            .checked_add(size_of::<ParametersData<'_>>())?
            .checked_add(2 * size_of::<usize>())?,
    })
}

pub(in crate::types) trait SignatureSourceEffects<'db> {
    type Error;

    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;
    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error>;
    async fn semantic_index(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<&'db SemanticIndex<'db>, Self::Error>;
    async fn parameter_annotation(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<(Type<'db>, TypeExpressionFlags), Self::Error>;
    async fn return_annotation(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error>;
    async fn unpacked_kwargs(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        flags: TypeExpressionFlags,
    ) -> Result<bool, Self::Error>;
    async fn extend_unpacked_kwargs(
        &self,
        db: &'db dyn Db,
        parameters: &mut Vec<Parameter<'db>>,
        parameter: &Parameter<'db>,
    ) -> Result<(), Self::Error>;
    async fn normalize_paramspec(
        &self,
        db: &'db dyn Db,
        args: BoundTypeVarInstance<'db>,
        kwargs: BoundTypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
    async fn legacy_generic_context(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameters: &Parameters<'db>,
        return_ty: Type<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error>;
    async fn merge_generic_contexts(
        &self,
        db: &'db dyn Db,
        pep695: GenericContext<'db>,
        legacy: GenericContext<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error>;
    async fn rescope_return_callables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        parameters: &Parameters<'db>,
        return_ty: Type<'db>,
        definition: Definition<'db>,
    ) -> Result<(Option<GenericContext<'db>>, Type<'db>), Self::Error>;
}

pub(in crate::types) struct InlineSignatureSourceEffects;

impl<'db> SignatureSourceEffects<'db> for InlineSignatureSourceEffects {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> Result<R::Output, Self::Error> {
        Ok(request.read_ordinary())
    }

    async fn semantic_index(
        &self,
        db: &'db dyn Db,
        file: ProgramFile<'db>,
    ) -> Result<&'db SemanticIndex<'db>, Self::Error> {
        Ok(semantic_index(db, file))
    }

    async fn parameter_annotation(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<(Type<'db>, TypeExpressionFlags), Self::Error> {
        Ok((
            function_signature_expression_type(db, definition, expression),
            function_signature_type_expression_flags(db, definition, expression),
        ))
    }

    async fn return_annotation(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(function_signature_expression_type(
            db, definition, expression,
        ))
    }

    async fn unpacked_kwargs(
        &self,
        db: &'db dyn Db,
        ty: Type<'db>,
        flags: TypeExpressionFlags,
    ) -> Result<bool, Self::Error> {
        Ok(extract_unpacked_typed_dict_keys_from_kwargs_annotation(db, ty, flags).is_some())
    }

    async fn extend_unpacked_kwargs(
        &self,
        db: &'db dyn Db,
        parameters: &mut Vec<Parameter<'db>>,
        parameter: &Parameter<'db>,
    ) -> Result<(), Self::Error> {
        if let Some(typed_dict) = parameter.unpacked_typed_dict(db) {
            Parameters::push_unpacked_typed_dict(db, parameters, parameter, typed_dict);
        } else {
            parameters.push(parameter.clone());
        }
        Ok(())
    }

    async fn normalize_paramspec(
        &self,
        db: &'db dyn Db,
        args: BoundTypeVarInstance<'db>,
        kwargs: BoundTypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error> {
        if let (Some(ParamSpecAttrKind::Args), Some(ParamSpecAttrKind::Kwargs)) =
            (args.paramspec_attr(db), kwargs.paramspec_attr(db))
        {
            let typevar = args.without_paramspec_attr(db);
            if typevar.is_same_typevar_as(db, kwargs.without_paramspec_attr(db)) {
                return Ok(Some(typevar));
            }
        }
        Ok(None)
    }

    async fn legacy_generic_context(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameters: &Parameters<'db>,
        return_ty: Type<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(GenericContext::from_function_params(
            db, definition, parameters, return_ty,
        ))
    }

    async fn merge_generic_contexts(
        &self,
        db: &'db dyn Db,
        pep695: GenericContext<'db>,
        legacy: GenericContext<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        Ok(GenericContext::merge_pep695_and_legacy(
            db,
            Some(pep695),
            Some(legacy),
        ))
    }

    async fn rescope_return_callables(
        &self,
        db: &'db dyn Db,
        context: GenericContext<'db>,
        parameters: &Parameters<'db>,
        return_ty: Type<'db>,
        definition: Definition<'db>,
    ) -> Result<(Option<GenericContext<'db>>, Type<'db>), Self::Error> {
        Ok(GenericContext::remove_callable_only_typevars(
            db,
            Some(context),
            parameters,
            return_ty,
            definition,
        ))
    }
}

impl<'db> Signature<'db> {
    /// Return a typed signature from a function definition.
    pub(in crate::types) async fn from_function_with<E: SignatureSourceEffects<'db>>(
        db: &'db dyn Db,
        pep695_generic_context: Option<GenericContext<'db>>,
        definition: Definition<'db>,
        function_node: &ast::StmtFunctionDef,
        has_implicitly_positional_first_parameter: bool,
        return_callable_typevar_scope: ReturnCallableTypeVarScope,
        effects: &E,
    ) -> Result<Self, E::Error> {
        let parameters = Parameters::from_parameters_with(
            db,
            definition,
            &function_node.parameters,
            has_implicitly_positional_first_parameter,
            effects,
        )
        .await?;
        let return_ty = if let Some(returns) = &function_node.returns {
            effects.return_annotation(db, definition, returns).await?
        } else {
            Type::unknown()
        };
        let legacy = effects
            .legacy_generic_context(db, definition, &parameters, return_ty)
            .await?;
        let full_context = match (pep695_generic_context, legacy) {
            (Some(pep695), Some(legacy)) => {
                effects.merge_generic_contexts(db, pep695, legacy).await?
            }
            (left, right) => left.or(right),
        };
        let (generic_context, return_ty) = match (return_callable_typevar_scope, full_context) {
            (ReturnCallableTypeVarScope::Public, Some(context)) => {
                effects
                    .rescope_return_callables(db, context, &parameters, return_ty, definition)
                    .await?
            }
            (_, context) => (context, return_ty),
        };
        Ok(Self {
            generic_context,
            definition: Some(definition),
            extras: None,
            parameters,
            return_ty,
            is_paramspec_value: false,
            is_recursion_recovery: false,
        })
    }

    pub(in crate::types) async fn with_source_overload_index_with<
        E: SignatureSourceEffects<'db>,
    >(
        mut self,
        index: Option<usize>,
        effects: &E,
    ) -> Result<Self, E::Error> {
        let bytes = if self.extras.is_none() && index.is_some() {
            size_of::<SignatureExtras<'db>>()
        } else {
            0
        };
        effects
            .local(Some(4), Some(bytes), || {
                let index = index
                    .and_then(|index| u32::try_from(index).ok())
                    .and_then(|index| index.checked_add(1))
                    .and_then(NonZeroU32::new);
                self.set_source_overload_index(index);
            })
            .await?;
        Ok(self)
    }
}

#[derive(Clone, Copy)]
enum SourceParameterKind {
    PositionalOnly,
    PositionalOrKeyword,
    Variadic,
    KeywordOnly,
    KeywordVariadic,
}

impl<'db> Parameters<'db> {
    async fn from_parameters_with<E: SignatureSourceEffects<'db>>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        parameters: &ast::Parameters,
        implicit_positional: bool,
        effects: &E,
    ) -> Result<Self, E::Error> {
        let positional_prefix = effects
            .local(parameters.args.len().checked_add(1), Some(0), || {
                if !parameters.posonlyargs.is_empty() {
                    return 0;
                }
                let implicit = usize::from(implicit_positional && !parameters.args.is_empty());
                implicit
                    + parameters
                        .args
                        .iter()
                        .skip(implicit)
                        .take_while(|parameter| parameter.uses_pep_484_positional_only_convention())
                        .count()
            })
            .await?;
        let mut positional_only = Vec::new();
        for (index, parameter) in parameters
            .posonlyargs
            .iter()
            .chain(parameters.args.iter().take(positional_prefix))
            .enumerate()
        {
            let parameter = Self::source_parameter_with(
                db,
                definition,
                &parameter.parameter,
                parameter.default().is_some(),
                SourceParameterKind::PositionalOnly,
                index,
                effects,
            )
            .await?;
            Self::push_parameter_with(&mut positional_only, parameter, effects).await?;
        }
        // The inline constructor computes these optional parameters before consuming the lazy
        // positional-or-keyword and keyword-only iterators. Preserve that dependency order.
        let variadic_index = parameters.posonlyargs.len() + parameters.args.len();
        let variadic = if let Some(parameter) = &parameters.vararg {
            Some(
                Self::source_parameter_with(
                    db,
                    definition,
                    parameter,
                    false,
                    SourceParameterKind::Variadic,
                    variadic_index,
                    effects,
                )
                .await?,
            )
        } else {
            None
        };
        let keyword_only_index = variadic_index + usize::from(parameters.vararg.is_some());
        let keywords = if let Some(parameter) = &parameters.kwarg {
            Some(
                Self::source_parameter_with(
                    db,
                    definition,
                    parameter,
                    false,
                    SourceParameterKind::KeywordVariadic,
                    keyword_only_index + parameters.kwonlyargs.len(),
                    effects,
                )
                .await?,
            )
        } else {
            None
        };
        let mut value = Vec::new();
        for parameter in positional_only {
            Self::push_normalized_parameter_with(db, &mut value, parameter, effects).await?;
        }
        for (index, parameter) in parameters.args.iter().enumerate().skip(positional_prefix) {
            let parameter = Self::source_parameter_with(
                db,
                definition,
                &parameter.parameter,
                parameter.default().is_some(),
                SourceParameterKind::PositionalOrKeyword,
                parameters.posonlyargs.len() + index,
                effects,
            )
            .await?;
            Self::push_normalized_parameter_with(db, &mut value, parameter, effects).await?;
        }
        if let Some(parameter) = variadic {
            Self::push_normalized_parameter_with(db, &mut value, parameter, effects).await?;
        }
        for (index, parameter) in parameters.kwonlyargs.iter().enumerate() {
            let parameter = Self::source_parameter_with(
                db,
                definition,
                &parameter.parameter,
                parameter.default().is_some(),
                SourceParameterKind::KeywordOnly,
                keyword_only_index + index,
                effects,
            )
            .await?;
            Self::push_normalized_parameter_with(db, &mut value, parameter, effects).await?;
        }
        if let Some(parameter) = keywords {
            Self::push_normalized_parameter_with(db, &mut value, parameter, effects).await?;
        }
        Self::from_normalized_with(db, value, effects).await
    }

    async fn source_parameter_with<E: SignatureSourceEffects<'db>>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        node: &ast::Parameter,
        has_default: bool,
        source_kind: SourceParameterKind,
        source_index: usize,
        effects: &E,
    ) -> Result<Parameter<'db>, E::Error> {
        let scope = effects.field(definition.read_fields(db).scope_id()).await?;
        let file = effects.field(scope.read_fields(db).program_file()).await?;
        let index = effects.semantic_index(db, file).await?;
        let default_type = if has_default {
            let work = effects
                .local(Some(1), Some(0), || index.definition_lookup_work())
                .await?;
            Some(
                effects
                    .local(work.checked_add(1), Some(0), || {
                        ParameterDefault::Deferred(index.expect_single_definition(node))
                    })
                    .await?,
            )
        } else {
            None
        };
        let kind = effects
            .local(Some(3), Some(0), || {
                let name = node.name.id.clone();
                match source_kind {
                    SourceParameterKind::PositionalOnly => ParameterKind::PositionalOnly {
                        name: Some(name),
                        default_type,
                    },
                    SourceParameterKind::PositionalOrKeyword => {
                        ParameterKind::PositionalOrKeyword { name, default_type }
                    }
                    SourceParameterKind::Variadic => ParameterKind::Variadic { name },
                    SourceParameterKind::KeywordOnly => {
                        ParameterKind::KeywordOnly { name, default_type }
                    }
                    SourceParameterKind::KeywordVariadic => ParameterKind::KeywordVariadic { name },
                }
            })
            .await?;
        Ok(
            Parameter::from_node_and_kind_with(db, definition, node, kind, effects)
                .await?
                .with_source_parameter_index(Some(source_index)),
        )
    }

    pub(in crate::types) async fn push_parameter_with<E: SignatureSourceEffects<'db>>(
        value: &mut Vec<Parameter<'db>>,
        parameter: Parameter<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        let grows = value.len() == value.capacity();
        let capacity = value
            .len()
            .checked_add(1)
            .and_then(usize::checked_next_power_of_two);
        let bytes = if grows {
            capacity.and_then(|n| n.checked_mul(size_of::<Parameter<'db>>()))
        } else {
            Some(0)
        };
        let work = if grows {
            value.len().checked_mul(8).and_then(|n| n.checked_add(10))
        } else {
            Some(10)
        };
        let mut parameter = Some(parameter);
        effects
            .local(work, bytes, || {
                if grows && let Some(capacity) = capacity {
                    value.reserve_exact(capacity - value.len());
                }
                value.extend(parameter.take());
            })
            .await
    }

    async fn push_normalized_parameter_with<E: SignatureSourceEffects<'db>>(
        db: &'db dyn Db,
        value: &mut Vec<Parameter<'db>>,
        parameter: Parameter<'db>,
        effects: &E,
    ) -> Result<(), E::Error> {
        if parameter.is_keyword_variadic()
            && parameter.annotation_kind == ParameterAnnotationKind::UnpackedTypedDictKwargs
        {
            effects.extend_unpacked_kwargs(db, value, &parameter).await
        } else {
            Self::push_parameter_with(value, parameter, effects).await
        }
    }

    pub(in crate::types) async fn from_annotation_with<E: SignatureSourceEffects<'db>>(
        db: &'db dyn Db,
        parameters: impl IntoIterator<Item = Parameter<'db>>,
        effects: &E,
    ) -> Result<Self, E::Error> {
        let mut value = Vec::new();
        for parameter in parameters {
            Self::push_normalized_parameter_with(db, &mut value, parameter, effects).await?;
        }
        Self::from_normalized_with(db, value, effects).await
    }

    async fn from_normalized_with<E: SignatureSourceEffects<'db>>(
        db: &'db dyn Db,
        value: Vec<Parameter<'db>>,
        effects: &E,
    ) -> Result<Self, E::Error> {
        let mut kind = ParametersKind::Standard;
        let (variadic, keyword_variadic) = effects
            .local(value.len().checked_mul(2), Some(0), || {
                (
                    value
                        .iter()
                        .enumerate()
                        .find(|(_, param)| param.is_variadic())
                        .map(|(index, param)| (index, param.annotated_type)),
                    value
                        .iter()
                        .enumerate()
                        .find(|(_, param)| param.is_keyword_variadic())
                        .map(|(index, param)| (index, param.annotated_type)),
                )
            })
            .await?;
        if let (Some((var_index, var_type)), Some((kw_index, kw_type))) =
            (variadic, keyword_variadic)
        {
            let prefix = value.get(..var_index).unwrap_or(&[]);
            let keyword_only = value.get(var_index + 1..kw_index).unwrap_or(&[]);
            match (var_type, kw_type) {
                (Type::Dynamic(_), Type::Dynamic(_)) => {
                    kind = effects
                        .local(prefix.len().checked_add(1), Some(0), || {
                            if keyword_only.is_empty()
                                && !prefix.is_empty()
                                && prefix.iter().all(Parameter::is_positional_only)
                            {
                                ParametersKind::Concatenate(ConcatenateTail::Gradual)
                            } else {
                                ParametersKind::Gradual
                            }
                        })
                        .await?;
                }
                (Type::TypeVar(args), Type::TypeVar(kwargs)) if keyword_only.is_empty() => {
                    if let Some(typevar) = effects.normalize_paramspec(db, args, kwargs).await? {
                        kind = effects
                            .local(prefix.len().checked_add(1), Some(0), || {
                                if prefix.is_empty() {
                                    ParametersKind::ParamSpec(typevar)
                                } else if prefix.iter().all(Parameter::is_positional) {
                                    ParametersKind::Concatenate(ConcatenateTail::ParamSpec(typevar))
                                } else {
                                    ParametersKind::Standard
                                }
                            })
                            .await?;
                    }
                }
                _ => {}
            }
        }
        let quote = parameters_storage_quote(value.len());
        let bytes = quote.map(|quote| quote.bytes);
        let work = quote.map(|quote| quote.work);
        let mut value = value;
        effects
            .local(work, bytes, || Self::new(std::mem::take(&mut value), kind))
            .await
    }
}

impl<'db> Parameter<'db> {
    async fn from_node_and_kind_with<E: SignatureSourceEffects<'db>>(
        db: &'db dyn Db,
        function_definition: Definition<'db>,
        parameter: &ast::Parameter,
        kind: ParameterKind<'db>,
        effects: &E,
    ) -> Result<Self, E::Error> {
        let scope = effects
            .field(function_definition.read_fields(db).scope_id())
            .await?;
        let file = effects.field(scope.read_fields(db).program_file()).await?;
        let index = effects.semantic_index(db, file).await?;
        let work = effects
            .local(Some(1), Some(0), || index.definition_lookup_work())
            .await?;
        let definition = effects
            .local(work.checked_add(1), Some(0), || {
                Some(index.expect_single_definition(parameter))
            })
            .await?;
        let (annotated_type, inferred_annotation, flags, starred) =
            if let Some(annotation) = parameter.annotation() {
                let (ty, flags) = effects
                    .parameter_annotation(db, function_definition, annotation)
                    .await?;
                (ty, false, flags, annotation.is_starred_expr())
            } else {
                (Type::unknown(), true, TypeExpressionFlags::empty(), false)
            };
        let unpacked_variadic = matches!(&kind, ParameterKind::Variadic { .. })
            && flags.contains(TypeExpressionFlags::UNPACK);
        let unpacked_kwargs = matches!(&kind, ParameterKind::KeywordVariadic { .. })
            && flags.contains(TypeExpressionFlags::UNPACK)
            && effects.unpacked_kwargs(db, annotated_type, flags).await?;
        let annotation_kind = if unpacked_kwargs {
            ParameterAnnotationKind::UnpackedTypedDictKwargs
        } else if starred || unpacked_variadic {
            ParameterAnnotationKind::Starred
        } else {
            ParameterAnnotationKind::Normal
        };
        Ok(Self {
            annotated_type,
            definition,
            inferred_annotation,
            annotation_kind,
            source_parameter_index: None,
            kind,
        })
    }
}
