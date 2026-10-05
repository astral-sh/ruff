//! Successful calls share diagnostic decisions before their return types are assembled.

use super::*;
use crate::lint::LintMetadata;
use crate::types::StaticClassLiteral;
use crate::types::class::CodeGeneratorKind;
use crate::types::diagnostic::PYDANTIC_DISCARDED_EXTRA_ARGUMENT;
use crate::types::call::bind::deprecation::{DeprecatedFunctions, DeprecationEffects};
use crate::types::function::{FunctionDecorators, LegacyFunctionIdentityEffects};

#[derive(Clone, Copy)]
pub(in crate::types::infer::builder::local) enum CompletionDependency {
    DeprecationDiagnostic,
    DiscardedExtraArguments,
    KnownFunction,
    KnownClass,
    NeverReveal,
    ReceiverConstraints,
    CallDiagnostic,
}

pub(in crate::types::infer::builder::local) trait CompletionEffects<'db>:
    DeprecationEffects<'db>
{
    async fn decision<T, F: FnOnce() -> T>(
        &self,
        work: Option<usize>,
        action: F,
    ) -> Result<T, Self::Error>;

    async fn begin_checks(&self, bindings: &Bindings<'db>) -> Result<(), Self::Error>;

    async fn lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
    ) -> Result<bool, Self::Error>;

    async fn static_class(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Self::Error>;

    async fn code_generator(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;

    async fn completion<T>(
        &self,
        dependency: CompletionDependency,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;
}

impl<'db> CompletionEffects<'db> for LegacyFunctionIdentityEffects {
    async fn decision<T, F: FnOnce() -> T>(
        &self,
        _work: Option<usize>,
        action: F,
    ) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn begin_checks(&self, _bindings: &Bindings<'db>) -> Result<(), Infallible> {
        Ok(())
    }

    async fn lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
    ) -> Result<bool, Infallible> {
        Ok(builder.context.is_lint_enabled(lint))
    }

    async fn static_class(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<StaticClassLiteral<'db>>, Infallible> {
        Ok(class.static_class_literal(db).map(|(class, _)| class))
    }

    async fn code_generator(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Infallible> {
        Ok(CodeGeneratorKind::from_class(db, class.into()))
    }

    async fn completion<T>(
        &self,
        _dependency: CompletionDependency,
        action: impl FnOnce() -> T,
    ) -> Result<T, Infallible> {
        Ok(action())
    }
}

pub(in crate::types::infer::builder::local) async fn report_failed_with<'db, E>(
    builder: &TypeInferenceBuilder<'db, '_>,
    data: &CallData<'db, '_>,
    bindings: &Bindings<'db>,
    effects: &E,
) -> Result<(), E::Error>
where
    E: CompletionEffects<'db>,
{
    effects
        .completion(CompletionDependency::CallDiagnostic, || {
            bindings.report_diagnostics(&builder.context, data.call.into());
        })
        .await
}

pub(in crate::types::infer::builder::local) async fn successful_checks_with<'db, E>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    data: &CallData<'db, '_>,
    call_arguments: &CallArguments<'_, 'db>,
    bindings: &mut Bindings<'db>,
    effects: &E,
) -> Result<(), E::Error>
where
    E: CompletionEffects<'db>,
{
    effects.begin_checks(bindings).await?;
    let db = builder.db();
    let call_expression = data.call;
    let func = &call_expression.func;
    let arguments = &call_expression.arguments;

    // Explicit function references already report implementation deprecations. Other calls
    // reference an object or class, rather than the method invoked implicitly.
    let is_function_reference = matches!(
        data.callable_type,
        Type::FunctionLiteral(_) | Type::BoundMethod(_) | Type::Callable(_)
    );
    {
        let mut deprecated = DeprecatedFunctions::default();
        bindings
            .collect_deprecated_functions_with(db, &mut deprecated, effects)
            .await?;
        effects
            .decision(deprecated.as_slice().len().checked_add(1), || ())
            .await?;
        let mut should_report = false;
        for (_, function) in deprecated.as_slice() {
            let decorators = effects
                .field(function.field_requests(db).decorators())
                .await?;
            if decorators.contains(FunctionDecorators::OVERLOAD) || !is_function_reference {
                should_report = true;
                break;
            }
        }
        if should_report {
            effects
                .completion(CompletionDependency::DeprecationDiagnostic, || {
                    builder.report_deprecated_functions(
                        func.as_ref(),
                        deprecated
                            .as_slice()
                            .iter()
                            .map(|(_, function)| *function)
                            .filter(|function| function.is_overload(db) || !is_function_reference),
                    );
                })
                .await?;
        }
    }

    if let Some(class) = data.class {
        discarded_extra_arguments_with(builder, class, arguments, bindings, effects).await?;
    }

    for binding in bindings.iter_flat_mut() {
        let binding_type = binding.callable_type;
        let matching_work = effects
            .decision(binding.overloads().len().checked_add(1), || {
                binding
                    .overloads()
                    .iter()
                    .try_fold(1usize, |work, overload| {
                        work.checked_add(overload.errors().len())?.checked_add(2)
                    })
            })
            .await?;
        effects.decision(matching_work, || ()).await?;
        for (_, overload) in binding.matching_overloads_mut() {
            match binding_type {
                Type::FunctionLiteral(function) => {
                    effects.decision(Some(2), || ()).await?;
                    let literal = effects.field(function.field_requests(db).literal()).await?;
                    let known = effects
                        .field(literal.last_definition.field_requests(db).known())
                        .await?;
                    let check = effects
                        .decision(Some(2), || known.and_then(KnownFunction::call_check))
                        .await?;
                    if let Some(check) = check {
                        effects
                            .completion(CompletionDependency::KnownFunction, || {
                                check.check_call(
                                    &builder.context,
                                    overload,
                                    call_arguments,
                                    call_expression,
                                    builder.index,
                                );
                            })
                            .await?;
                    }
                }
                Type::ClassLiteral(class) => {
                    effects.decision(Some(2), || ()).await?;
                    let known = match class.as_static() {
                        Some(class) => effects.field(class.field_requests(db).known()).await?,
                        None => None,
                    };
                    let check = effects
                        .decision(Some(2), || known.and_then(KnownClass::call_check))
                        .await?;
                    if let Some(check) = check {
                        effects
                            .completion(CompletionDependency::KnownClass, || {
                                check.check_call(
                                    &builder.context,
                                    builder.index,
                                    overload,
                                    call_expression,
                                );
                            })
                            .await?;
                    }
                }
                Type::Never => {
                    // Unreachable references still report reveal_type calls. Their inferred
                    // callable is Never, so identify these calls from the expression's name.
                    let is_reveal_type = effects
                        .decision(
                            (u32::from(func.range().len()) as usize).checked_add(4),
                            || match func.as_ref() {
                                ast::Expr::Name(name) => name.id == "reveal_type",
                                ast::Expr::Attribute(attr) => {
                                    attr.attr.id == "reveal_type" && is_dotted_name(func)
                                }
                                _ => false,
                            },
                        )
                        .await?;
                    if is_reveal_type && let Some(first_arg) = arguments.args.first() {
                        effects
                            .completion(CompletionDependency::NeverReveal, || {
                                let revealed_ty = builder.expression_type(first_arg);
                                report_revealed_type(&builder.context, revealed_ty, first_arg);
                            })
                            .await?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

async fn discarded_extra_arguments_with<'db, E: CompletionEffects<'db>>(
    builder: &TypeInferenceBuilder<'db, '_>,
    class: ClassType<'db>,
    arguments: &ast::Arguments,
    bindings: &Bindings<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    if !effects
        .lint_enabled(builder, &PYDANTIC_DISCARDED_EXTRA_ARGUMENT)
        .await?
    {
        return Ok(());
    }
    let Some(class) = effects.static_class(builder.db(), class).await? else {
        return Ok(());
    };
    let generator = effects.code_generator(builder.db(), class).await?;
    let Some(metadata) = effects
        .decision(Some(2), || generator.and_then(CodeGeneratorKind::pydantic_metadata))
        .await?
    else {
        return Ok(());
    };
    effects
        .completion(CompletionDependency::DiscardedExtraArguments, || {
            pydantic::report_discarded_extra_arguments(
                &builder.context, class, metadata, arguments, bindings,
            );
        })
        .await
}

pub(in crate::types::infer::builder::local) async fn receiver_constraints_with<'db, E>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    data: &CallData<'db, '_>,
    call_arguments: &mut CallArguments<'_, 'db>,
    effects: &E,
) -> Result<(), E::Error>
where
    E: CompletionEffects<'db>,
{
    effects.decision(Some(1), || ()).await?;
    if let ast::Expr::Attribute(attribute @ ast::ExprAttribute { value, .. }) =
        data.call.func.as_ref()
    {
        let value_type = effects
            .decision(builder.expressions.capacity().checked_add(2), || {
                builder.expression_type(value)
            })
            .await?;
        let collection_def = effects
            .decision(Some(4), || {
                builder.index.unannotated_collection_initializer(value)
            })
            .await?;
        if let Some(collection_def) = collection_def {
            effects
                .completion(CompletionDependency::ReceiverConstraints, || {
                    builder.local_receiver_collection_constraints(
                        data,
                        call_arguments,
                        attribute,
                        value_type,
                        collection_def,
                    );
                })
                .await?;
        }
    }
    Ok(())
}
