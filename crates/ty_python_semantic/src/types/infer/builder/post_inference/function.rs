use crate::{
    Db,
    diagnostic::format_enumeration,
    types::{
        BoundTypeVarIdentity, KnownInstanceType, Signature, StaticClassLiteral, Type, TypeVarKind,
        TypeVarVariance,
        context::InferContext,
        diagnostic::{
            INVALID_GENERIC_CLASS, INVALID_LEGACY_POSITIONAL_PARAMETER,
            INVALID_TYPE_VARIABLE_DEFAULT, UNBOUND_TYPE_VARIABLE,
        },
        function::{FunctionDecorators, FunctionType, OverloadLiteral},
        generics::GenericContext,
        list_members::all_end_of_scope_members,
        member::class_member,
        signatures::ReturnCallableTypeVarScope,
        typevar::TypeVarInstance,
        variance::{MemberVariance, VarianceInferable},
        visitor::find_over_type,
    },
};
use itertools::Itertools;
use ruff_db::{
    diagnostic::{Annotation, Span},
    parsed::parsed_module,
};
use ruff_python_ast as ast;
use ruff_text_size::{Ranged, TextRange};
use rustc_hash::FxHashSet;
use ty_python_core::definition::Definition;

pub(crate) fn check_function_definition<'db>(
    context: &InferContext<'db, '_>,
    definition: Definition<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    match source_effects::check_function_definition_sync(
        definition,
        source_effects::FunctionFacts,
        &source_effects::OrdinaryFunctionEffects {
            context,
            file_expression_type,
        },
    ) {
        Ok(()) => {}
        Err(error) => match error {},
    }
}

pub(in crate::types::infer::builder) mod source_effects {
    use std::convert::Infallible;

    use ruff_python_ast as ast;
    use ty_python_core::definition::Definition;

    use crate::types::context::InferContext;
    use crate::types::function::{FunctionDecorators, OverloadLiteral};
    use crate::types::signatures::{Parameter, ReturnCallableTypeVarScope};
    use crate::types::{Signature, Type, infer_definition_types};

    pub(in crate::types::infer::builder) type ParameterCursor<'a, 'db> =
        std::iter::Zip<ast::ParametersIterator<'a>, std::slice::Iter<'a, Parameter<'db>>>;

    pub(in crate::types::infer::builder) struct FunctionFacts;

    pub(super) struct OrdinaryFunctionEffects<'a, 'db, 'ast, F> {
        pub(super) context: &'a InferContext<'db, 'ast>,
        pub(super) file_expression_type: &'a F,
    }

    ty_mapping_probe_macros::shared_semantic_family! {
        #[synchronous(SynchronousFunctionEffects)]
        pub(in crate::types::infer::builder) trait FunctionEffects<'db> {
            type Error;

            #[operation(child)]
            async fn canonical_last_definition(
                &self,
                definition: Definition<'db>,
            ) -> Result<Option<OverloadLiteral<'db>>, Self::Error>;

            #[operation(child)]
            async fn has_no_type_check(
                &self,
                last_definition: OverloadLiteral<'db>,
            ) -> Result<bool, Self::Error>;

            #[operation(child)]
            async fn raw_public_signature(
                &self,
                last_definition: OverloadLiteral<'db>,
            ) -> Result<Signature<'db>, Self::Error>;

            #[operation(source)]
            async fn function_node<'a>(
                &'a self,
                last_definition: OverloadLiteral<'db>,
            ) -> Result<&'a ast::StmtFunctionDef, Self::Error>;

            #[operation(local)]
            async fn parameter_cursor<'a>(
                &self,
                parameters: &'a ast::Parameters,
                signature: &'a Signature<'db>,
            ) -> Result<ParameterCursor<'a, 'db>, Self::Error>;

            #[operation(local)]
            #[progress]
            async fn next_parameter<'a>(
                &self,
                cursor: &mut ParameterCursor<'a, 'db>,
            ) -> Result<Option<(ast::AnyParameterRef<'a>, &'a Parameter<'db>)>, Self::Error>;

            #[operation(source)]
            async fn report_invalid_legacy_positional(
                &self,
                parameter: &ast::ParameterWithDefault,
                previous: Option<&ast::ParameterWithDefault>,
            ) -> Result<(), Self::Error>;

            #[operation(source)]
            async fn check_pep695_legacy_typevars(
                &self,
                last_definition: OverloadLiteral<'db>,
            ) -> Result<(), Self::Error>;

            #[operation(source)]
            async fn check_legacy_typevar_defaults(
                &self,
                last_definition: OverloadLiteral<'db>,
                signature: &Signature<'db>,
            ) -> Result<(), Self::Error>;

            #[operation(source)]
            async fn check_legacy_typevar_ordering(
                &self,
                last_definition: OverloadLiteral<'db>,
                signature: &Signature<'db>,
            ) -> Result<(), Self::Error>;

            #[operation(local)]
            async fn retire_signature(&self, signature: Signature<'db>) -> Result<(), Self::Error>;
        }

        #[finite_capability]
        impl FunctionFacts {
            fn has_pep570_parameters(&self, node: &ast::StmtFunctionDef) -> bool {
                !node.parameters.posonlyargs.is_empty()
            }

            fn is_positional_only(&self, parameter: &Parameter<'_>) -> bool {
                parameter.is_positional_only()
            }

            fn uses_legacy_positional_convention(&self, parameter: &ast::ParameterWithDefault) -> bool {
                parameter.uses_pep_484_positional_only_convention()
            }

            fn has_previous_parameter(&self, previous: Option<&ast::ParameterWithDefault>) -> bool {
                previous.is_some()
            }

            fn has_pep695_parameters(&self, node: &ast::StmtFunctionDef) -> bool {
                node.type_params.is_some()
            }

            fn has_generic_context(&self, signature: &Signature<'_>) -> bool {
                signature.generic_context.is_some()
            }
        }

        #[synchronous(check_function_definition_sync)]
        #[capabilities(effects = FunctionEffects, facts = FunctionFacts)]
        #[passive_values()]
        pub(in crate::types::infer::builder) async fn check_function_definition_with<'db, E: FunctionEffects<'db>>(
            definition: Definition<'db>,
            facts: FunctionFacts,
            effects: &E,
        ) -> Result<(), E::Error> {
            let Some(last_definition) = effects.canonical_last_definition(definition).await? else {
                return Ok(());
            };
            if effects.has_no_type_check(last_definition).await? {
                return Ok(());
            }
            let signature = effects.raw_public_signature(last_definition).await?;
            let node = effects.function_node(last_definition).await?;

            // If the function has any PEP-570 positional-only parameters,
            // assume that `__`-prefixed parameters are not meant to be positional-only.
            if !facts.has_pep570_parameters(node) {
                let mut cursor = effects.parameter_cursor(&node.parameters, &signature).await?;
                #[passive_state]
                let mut previous_non_positional_only = None;
                #[cursor_loop]
                while let Some(entry) = effects.next_parameter(&mut cursor).await? {
                    let (parameter_node, parameter) = entry;
                    let ast::AnyParameterRef::NonVariadic(parameter_node) = parameter_node else {
                        continue;
                    };
                    if facts.is_positional_only(parameter) {
                        continue;
                    }

                    // Valid uses of the PEP-484 positional-only convention will have been detected
                    // in the first iteration over this scope, so `is_positional_only()` returns true
                    // for those. Only invalid uses of the convention reach this check.
                    if facts.uses_legacy_positional_convention(parameter_node) {
                        effects.report_invalid_legacy_positional(parameter_node, previous_non_positional_only).await?;
                    } else if !facts.has_previous_parameter(previous_non_positional_only) {
                        previous_non_positional_only = Some(parameter_node);
                    }
                }
            }

            if facts.has_pep695_parameters(node) {
                effects.check_pep695_legacy_typevars(last_definition).await?;
            }
            if facts.has_generic_context(&signature) {
                effects.check_legacy_typevar_defaults(last_definition, &signature).await?;
                effects.check_legacy_typevar_ordering(last_definition, &signature).await?;
            }
            effects.retire_signature(signature).await?;
            Ok(())
        }
    }

    pub(in crate::types::infer::builder) fn parameter_cursor<'a, 'db>(
        parameters: &'a ast::Parameters,
        signature: &'a Signature<'db>,
    ) -> ParameterCursor<'a, 'db> {
        parameters.iter().zip(signature.parameters().iter())
    }

    pub(in crate::types::infer::builder) fn next_parameter<'a, 'db>(
        cursor: &mut ParameterCursor<'a, 'db>,
    ) -> Option<(ast::AnyParameterRef<'a>, &'a Parameter<'db>)> {
        cursor.next()
    }

    impl<'db, F: Fn(&ast::Expr) -> Type<'db>> SynchronousFunctionEffects<'db>
        for OrdinaryFunctionEffects<'_, 'db, '_, F>
    {
        type Error = Infallible;

        fn canonical_last_definition(
            &self,
            definition: Definition<'db>,
        ) -> Result<Option<OverloadLiteral<'db>>, Self::Error> {
            Ok(infer_definition_types(self.context.db(), definition)
                .function_type(definition)
                .map(|function| function.literal(self.context.db()).last_definition))
        }

        fn has_no_type_check(
            &self,
            last_definition: OverloadLiteral<'db>,
        ) -> Result<bool, Self::Error> {
            Ok(last_definition
                .has_known_decorator(self.context.db(), FunctionDecorators::NO_TYPE_CHECK))
        }

        fn raw_public_signature(
            &self,
            last_definition: OverloadLiteral<'db>,
        ) -> Result<Signature<'db>, Self::Error> {
            Ok(
                last_definition
                    .raw_signature(self.context.db(), ReturnCallableTypeVarScope::Public),
            )
        }

        fn function_node<'a>(
            &'a self,
            last_definition: OverloadLiteral<'db>,
        ) -> Result<&'a ast::StmtFunctionDef, Self::Error> {
            Ok(last_definition.node(
                self.context.db(),
                self.context.file(),
                self.context.module(),
            ))
        }

        fn parameter_cursor<'a>(
            &self,
            parameters: &'a ast::Parameters,
            signature: &'a Signature<'db>,
        ) -> Result<ParameterCursor<'a, 'db>, Self::Error> {
            Ok(parameter_cursor(parameters, signature))
        }

        fn next_parameter<'a>(
            &self,
            cursor: &mut ParameterCursor<'a, 'db>,
        ) -> Result<Option<(ast::AnyParameterRef<'a>, &'a Parameter<'db>)>, Self::Error> {
            Ok(next_parameter(cursor))
        }

        fn report_invalid_legacy_positional(
            &self,
            parameter: &ast::ParameterWithDefault,
            previous: Option<&ast::ParameterWithDefault>,
        ) -> Result<(), Self::Error> {
            super::report_invalid_legacy_positional(self.context, parameter, previous);
            Ok(())
        }

        fn check_pep695_legacy_typevars(
            &self,
            last_definition: OverloadLiteral<'db>,
        ) -> Result<(), Self::Error> {
            super::check_pep695_function_legacy_typevars(
                self.context,
                last_definition,
                self.file_expression_type,
            );
            Ok(())
        }

        fn check_legacy_typevar_defaults(
            &self,
            last_definition: OverloadLiteral<'db>,
            signature: &Signature<'db>,
        ) -> Result<(), Self::Error> {
            super::check_legacy_typevar_defaults(
                self.context,
                last_definition,
                signature,
                self.file_expression_type,
            );
            Ok(())
        }

        fn check_legacy_typevar_ordering(
            &self,
            last_definition: OverloadLiteral<'db>,
            signature: &Signature<'db>,
        ) -> Result<(), Self::Error> {
            super::check_legacy_typevar_ordering(
                self.context,
                last_definition,
                signature,
                self.file_expression_type,
            );
            Ok(())
        }

        fn retire_signature(&self, signature: Signature<'db>) -> Result<(), Self::Error> {
            drop(signature);
            Ok(())
        }
    }
}

/// Check that a nominal class's exposed methods respect its declared type-parameter variance.
/// Constructors are excluded because their parameters establish the class specialization.
/// Recursively checks type variables nested in containers, unions, and callables as well as bare uses.
pub(super) fn check_class_method_typevar_variance<'db>(
    context: &InferContext<'db, '_>,
    class: StaticClassLiteral<'db>,
    generic_context: GenericContext<'db>,
) {
    let db = context.db();

    // Protocols require declared variance to match the inferred variance, including for explicitly
    // invariant type variables. Nominal classes can be more conservative, so they only reject uses
    // incompatible with a declared covariance or contravariance. Both checks share recursive
    // variance inference, but only nominal classes currently skip overloads and independently
    // generic methods to avoid false positives.
    // TODO: Handle these cases in shared variance inference so both checks can account for them.
    if !generic_context.variables(db).any(|typevar| {
        matches!(
            typevar.typevar(db).explicit_variance(db),
            Some(TypeVarVariance::Covariant | TypeVarVariance::Contravariant)
        )
    }) {
        return;
    }

    let env = context.program_environment();
    let instance = class.variance_receiver(db, env);
    let mut reported = FxHashSet::default();
    for member in all_end_of_scope_members(db, class.body_scope(db))
        .unique_by(|member| member.member.name.clone())
    {
        let mut member = member.member;
        if matches!(member.name.as_str(), "__init__" | "__new__") {
            continue;
        }
        // The iterator lists declarations and bindings separately; lookup combines their types.
        let Some(ty) =
            class_member(db, class.body_scope(db), &member.name).ignore_possibly_undefined()
        else {
            continue;
        };
        member.ty = ty.resolve_type_alias(db);
        if let Type::PropertyInstance(property) = member.ty {
            // Each retained accessor has its own exclusions. Checking bound accessor signatures
            // includes the setter's input, which an ordinary property read would not expose.
            for (accessor, function) in property.accessors_with_functions(db) {
                if function.definition(db).scope(db) == class.body_scope(db)
                    && !exclude_from_variance(db, function)
                {
                    check_method_typevar_variance(
                        context,
                        generic_context,
                        function,
                        MemberVariance::accessor(db, env, accessor, instance),
                        &mut reported,
                    );
                }
            }
            continue;
        }
        for (function, ty) in member
            .local_function_bindings(db, class.body_scope(db))
            .filter(|(function, _)| !exclude_from_variance(db, *function))
        {
            check_method_typevar_variance(
                context,
                generic_context,
                function,
                MemberVariance::of(db, env, ty, instance),
                &mut reported,
            );
        }
    }
}

/// Whether a source method is exempt from declared-variance validation.
fn exclude_from_variance<'db>(db: &'db dyn Db, function: FunctionType<'db>) -> bool {
    let last_definition = function.literal(db).last_definition;
    // Variance depends on the complete overload set: a broader overload can cover an otherwise
    // incompatible signature.
    // TODO: Account for that coverage in shared variance inference before
    // checking overloaded methods here.
    if function.has_known_decorator(db, FunctionDecorators::OVERLOAD)
        || last_definition.has_known_decorator(db, FunctionDecorators::NO_TYPE_CHECK)
    {
        return true;
    }

    // Independent method type parameters can make an occurrence of a class parameter redundant.
    // TODO: Account for those relationships instead of just composing each occurrence's variance.
    // Use the lexical context so that type parameters moved into a returned callable also count.
    let lexical_signature = last_definition.raw_signature(db, ReturnCallableTypeVarScope::Lexical);
    lexical_signature.generic_context.is_some_and(|context| {
        context
            .variables(db)
            .any(|typevar| !typevar.typevar(db).is_self(db))
    })
}

fn check_method_typevar_variance<'db>(
    context: &InferContext<'db, '_>,
    generic_context: GenericContext<'db>,
    function: FunctionType<'db>,
    member: MemberVariance<'db>,
    reported: &mut FxHashSet<(Definition<'db>, BoundTypeVarIdentity<'db>)>,
) {
    let db = context.db();
    let env = context.program_environment();
    let last_definition = function.literal(db).last_definition;
    let signatures = match member.read_ty {
        Type::FunctionLiteral(function) => Some(function.signature(db)),
        Type::BoundMethod(method) => method.bound_signatures(db),
        Type::Callable(callable) => Some(callable.signatures(db)),
        _ => None,
    }
    .map(|signatures| signatures.overloads.as_slice());
    let signature = match signatures {
        Some([signature]) => Some(signature),
        Some(_) => return,
        None => None,
    }
    .filter(|signature| signature.definition() == Some(function.definition(db)));

    for typevar in generic_context.variables(db) {
        let Some(declared_variance) = typevar.typevar(db).explicit_variance(db) else {
            continue;
        };
        if declared_variance == TypeVarVariance::Invariant {
            continue;
        }
        let required_variance = member
            .variance_of(db, env, typevar.identity(db))
            .evaluate(db);
        if declared_variance.join(required_variance) == declared_variance
            || !reported.insert((function.definition(db), typevar.identity(db)))
        {
            continue;
        }
        let node = last_definition.node(db, context.file(), context.module());
        let range = signature
            .and_then(|signature| {
                let parameters = signature.parameters().iter().filter_map(|parameter| {
                    let annotation = node
                        .parameters
                        .iter()
                        .nth(parameter.source_parameter_index()?)?
                        .annotation()?;
                    // `P.args` and `P.kwargs` both consume `P`, despite having distinct identities.
                    let parameter_type = match parameter.annotated_type() {
                        Type::TypeVar(typevar) if typevar.paramspec_attr(db).is_some() => {
                            Type::TypeVar(typevar.without_paramspec_attr(db))
                        }
                        ty => ty,
                    };
                    Some((
                        annotation.range(),
                        parameter_type
                            .with_polarity(TypeVarVariance::Contravariant)
                            .variance_of(db, env, typevar.identity(db))
                            .evaluate(db),
                    ))
                });
                let returns = node.returns.iter().map(|annotation| {
                    (
                        annotation.range(),
                        signature
                            .return_ty
                            .variance_of(db, env, typevar.identity(db))
                            .evaluate(db),
                    )
                });
                parameters.chain(returns).find_map(|(range, variance)| {
                    (declared_variance.join(variance) != declared_variance).then_some(range)
                })
            })
            .unwrap_or_else(|| node.name.range());
        if let Some(builder) = context.report_lint(&INVALID_GENERIC_CLASS, range) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Variance of type variable `{}` is incompatible with method `{}`",
                typevar.name(db),
                node.name,
            ));
            diagnostic.info(format_args!(
                "Type variable `{}` is declared as {}, but this method requires it to be {}",
                typevar.name(db),
                declared_variance.as_str(),
                required_variance.as_str(),
            ));
        }
    }
}

/// Check that a function using PEP 695 syntax does not also introduce legacy type variables.
fn check_pep695_function_legacy_typevars<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();
    let node = last_definition.node(db, context.file(), context.module());
    let Some(type_params) = node.type_params.as_deref() else {
        return;
    };
    let env = context.program_environment();
    let mut has_legacy_default = false;
    for default in type_params.iter().filter_map(ast::TypeParam::default) {
        let Some(typevar) = find_over_type(db, env, file_expression_type(default), false, |ty| {
            if let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) = ty
                && matches!(
                    typevar.kind(db),
                    TypeVarKind::LegacyTypeVar
                        | TypeVarKind::Pep613Alias
                        | TypeVarKind::LegacyParamSpec
                )
            {
                Some(typevar)
            } else {
                None
            }
        }) else {
            continue;
        };

        report_pep695_function_legacy_typevar(context, typevar, default.range());
        has_legacy_default = true;
    }
    if has_legacy_default {
        return;
    }

    let signature = last_definition.raw_signature(db, ReturnCallableTypeVarScope::Lexical);
    let Some(definition) = signature.definition() else {
        return;
    };
    let Some(legacy_context) = GenericContext::from_function_params(
        db,
        definition,
        signature.parameters(),
        signature.return_ty,
    ) else {
        return;
    };

    for typevar in legacy_context
        .variables(db)
        .map(|typevar| typevar.typevar(db))
        .filter(|typevar| !typevar.is_self(db))
    {
        let range = find_typevar_annotation_range(context, node, typevar, file_expression_type);
        report_pep695_function_legacy_typevar(context, typevar, range);
    }
}

fn report_pep695_function_legacy_typevar<'db>(
    context: &InferContext<'db, '_>,
    typevar: TypeVarInstance<'db>,
    range: TextRange,
) {
    let db = context.db();
    if let Some(builder) = context.report_lint(&UNBOUND_TYPE_VARIABLE, range) {
        builder.into_diagnostic(format_args!(
            "Legacy type variable `{}` cannot be used in a function with PEP 695 type parameters",
            typevar.name(db),
        ));
    }
}

fn report_invalid_legacy_positional(
    context: &InferContext<'_, '_>,
    parameter: &ast::ParameterWithDefault,
    previous: Option<&ast::ParameterWithDefault>,
) {
    let Some(builder) = context.report_lint(&INVALID_LEGACY_POSITIONAL_PARAMETER, parameter.name())
    else {
        return;
    };
    let mut diagnostic = builder.into_diagnostic(
        "Invalid use of the legacy convention \
            for positional-only parameters",
    );
    diagnostic.set_primary_annotation_message(
        "Parameter name begins with `__` but will not be treated as positional-only",
    );
    diagnostic.info(
        "A parameter can only be positional-only \
            if it precedes all positional-or-keyword parameters",
    );
    if let Some(earlier_node) = previous {
        diagnostic.annotate(
            context
                .secondary(earlier_node.name())
                .message("Prior parameter here was positional-or-keyword"),
        );
    }
}

/// Check whether any legacy `TypeVar` used in a function signature has a default
/// that references an out-of-scope type variable.
///
/// This check mirrors the class-level check at `report_invalid_typevar_default_reference`,
/// but for function/method generic contexts.
fn check_legacy_typevar_defaults<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    let db = context.db();

    let Some(generic_context) = signature.generic_context else {
        return;
    };

    let env = context.program_environment();

    let typevars = generic_context
        .variables(db)
        .map(|bound_tvar| bound_tvar.typevar(db));

    for (i, typevar) in typevars.clone().enumerate() {
        // Only check legacy TypeVars; PEP 695 type parameters are already validated
        // by `check_default_for_outer_scope_typevars` in the type parameter scope.
        if !matches!(
            typevar.kind(db),
            TypeVarKind::LegacyTypeVar
                | TypeVarKind::Pep613Alias
                | TypeVarKind::LegacyParamSpec
                | TypeVarKind::LegacyTypeVarTuple
        ) {
            continue;
        }

        let Some(default_ty) = typevar.default_type(db, env) else {
            continue;
        };

        let first_bad_tvar = find_over_type(db, env, default_ty, false, |t| {
            let tvar = match t {
                Type::TypeVar(tvar) => tvar.typevar(db),
                Type::KnownInstance(KnownInstanceType::TypeVar(tvar)) => tvar,
                _ => return None,
            };
            if !typevars.clone().take(i).contains(&tvar) {
                Some(tvar)
            } else {
                None
            }
        });

        let Some(bad_typevar) = first_bad_tvar else {
            continue;
        };

        let is_later_in_list = typevars.clone().skip(i).contains(&bad_typevar);
        let node = last_definition.node(db, context.file(), context.module());

        let primary_range =
            find_typevar_annotation_range(context, node, typevar, file_expression_type);

        let Some(builder) = context.report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, primary_range)
        else {
            continue;
        };
        let typevar_name = typevar.name(db);
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Invalid use of type variable `{typevar_name}`",
        ));

        if is_later_in_list {
            diagnostic.set_primary_annotation_message(format_args!(
                "Default of `{typevar_name}` references later type parameter `{}`",
                bad_typevar.name(db),
            ));
            diagnostic.set_concise_message(format_args!(
                "Invalid use of type variable `{typevar_name}`: default of `{typevar_name}` \
                    refers to later parameter `{}`",
                bad_typevar.name(db)
            ));
        } else {
            diagnostic.set_primary_annotation_message(format_args!(
                "Default of `{typevar_name}` references out-of-scope type variable `{}`",
                bad_typevar.name(db),
            ));
            diagnostic.set_concise_message(format_args!(
                "Invalid use of type variable `{typevar_name}`: default of `{typevar_name}` \
                    refers to out-of-scope type variable `{}`",
                bad_typevar.name(db)
            ));
        }

        if let Some(typevar_definition) = typevar.definition(db) {
            diagnostic.annotate(
                Annotation::secondary(Span::from(typevar_definition.full_range(
                    db,
                    &parsed_module(db, typevar_definition.python_file(db)).load(db),
                )))
                .message(format_args!("`{typevar_name}` defined here")),
            );
        }

        diagnostic.info("See https://typing.python.org/en/latest/spec/generics.html#scoping-rules");
    }
}

fn find_typevar_annotation_range<'db>(
    context: &InferContext<'db, '_>,
    node: &ast::StmtFunctionDef,
    typevar: TypeVarInstance<'db>,
    file_expression_type: impl Fn(&ast::Expr) -> Type<'db>,
) -> TextRange {
    let db = context.db();
    let env = context.program_environment();
    let typevar_id = typevar.identity(db);

    node.parameters
        .iter()
        .filter_map(ast::AnyParameterRef::annotation)
        .chain(node.returns.as_deref())
        .find(|ann| file_expression_type(ann).references_typevar(db, env, typevar_id))
        .map(Ranged::range)
        .unwrap_or_else(|| node.name.range())
}

/// Check that legacy `TypeVar`s without defaults don't follow `TypeVar`s with defaults
/// in a function's generic context.
///
/// This mirrors the class-level check using `report_invalid_type_param_order`, but for
/// function/method generic contexts using the `invalid-type-variable-default` lint.
fn check_legacy_typevar_ordering<'db>(
    context: &InferContext<'db, '_>,
    last_definition: OverloadLiteral<'db>,
    signature: &Signature<'db>,
    file_expression_type: &impl Fn(&ast::Expr) -> Type<'db>,
) {
    struct State<'db> {
        typevar_with_default: TypeVarInstance<'db>,
        invalid_later_tvars: Vec<TypeVarInstance<'db>>,
    }

    let db = context.db();

    let Some(generic_context) = signature.generic_context else {
        return;
    };

    let env = context.program_environment();

    let mut state: Option<State<'db>> = None;

    for bound_typevar in generic_context.variables(db) {
        let typevar = bound_typevar.typevar(db);

        // Only check legacy TypeVars; PEP 695 ordering is validated by the parser.
        if !matches!(
            typevar.kind(db),
            TypeVarKind::LegacyTypeVar
                | TypeVarKind::Pep613Alias
                | TypeVarKind::LegacyParamSpec
                | TypeVarKind::LegacyTypeVarTuple
        ) {
            continue;
        }

        let has_default = typevar.default_type(db, env).is_some();

        if let Some(state) = state.as_mut() {
            if !has_default {
                state.invalid_later_tvars.push(typevar);
            }
        } else if has_default {
            state = Some(State {
                typevar_with_default: typevar,
                invalid_later_tvars: vec![],
            });
        }
    }

    let Some(state) = state else {
        return;
    };

    if state.invalid_later_tvars.is_empty() {
        return;
    }

    let node = last_definition.node(db, context.file(), context.module());

    let primary_range = find_typevar_annotation_range(
        context,
        node,
        state.invalid_later_tvars[0],
        file_expression_type,
    );

    let Some(builder) = context.report_lint(&INVALID_TYPE_VARIABLE_DEFAULT, primary_range) else {
        return;
    };

    let mut diagnostic = builder.into_diagnostic(
        "Type parameters without defaults cannot follow type parameters with defaults",
    );

    let typevar_with_default_name = state.typevar_with_default.name(db);

    diagnostic.set_concise_message(format_args!(
        "Type parameter `{}` without a default cannot follow \
            earlier parameter `{typevar_with_default_name}` with a default",
        state.invalid_later_tvars[0].name(db),
    ));

    if let [single_typevar] = &*state.invalid_later_tvars {
        diagnostic.set_primary_annotation_message(format_args!(
            "Type variable `{}` does not have a default",
            single_typevar.name(db),
        ));
    } else {
        let later_typevars =
            format_enumeration(state.invalid_later_tvars.iter().map(|tv| tv.name(db)));
        diagnostic.set_primary_annotation_message(format_args!(
            "Type variables {later_typevars} do not have defaults",
        ));
    }

    let secondary_range = find_typevar_annotation_range(
        context,
        node,
        state.typevar_with_default,
        file_expression_type,
    );

    diagnostic.annotate(context.secondary(secondary_range).message(format_args!(
        "Earlier TypeVar `{typevar_with_default_name}` has a default"
    )));

    for tvar in [state.typevar_with_default, state.invalid_later_tvars[0]] {
        let Some(definition) = tvar.definition(db) else {
            continue;
        };
        diagnostic.annotate(
            Annotation::secondary(Span::from(
                definition.full_range(db, &parsed_module(db, definition.python_file(db)).load(db)),
            ))
            .message(format_args!("`{}` defined here", tvar.name(db))),
        );
    }
}
