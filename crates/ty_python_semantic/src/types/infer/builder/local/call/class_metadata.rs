//! Class calls share protocol, abstractness, and type-definition context checks.

use super::*;
use crate::lint::LintMetadata;
use crate::types::abstract_methods::AbstractMethods;
use crate::types::protocol_class::ProtocolClass;

pub(in crate::types::infer::builder::local) trait ClassMetadataEffects<'db> {
    type Error;

    async fn decision<T>(&self, work: usize, action: impl FnOnce() -> T) -> Result<T, Self::Error>;
    async fn lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
    ) -> Result<bool, Self::Error>;
    async fn protocol_class(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<ProtocolClass<'db>>, Self::Error>;
    async fn abstract_methods(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<AbstractMethods<'db>, Self::Error>;
    async fn known(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error>;
    async fn diagnostic<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error>;
}

impl<'db> ClassMetadataEffects<'db> for OrdinaryCallEffects {
    type Error = Infallible;

    async fn decision<T>(&self, _work: usize, action: impl FnOnce() -> T) -> Result<T, Infallible> {
        Ok(action())
    }

    async fn lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
    ) -> Result<bool, Infallible> {
        Ok(builder.context.is_lint_enabled(lint))
    }

    async fn protocol_class(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<ProtocolClass<'db>>, Infallible> {
        Ok(class.into_protocol_class(db))
    }

    async fn abstract_methods(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<AbstractMethods<'db>, Infallible> {
        Ok(AbstractMethods::of_class(db, class))
    }

    async fn known(
        &self,
        db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> Result<Option<KnownClass>, Infallible> {
        Ok(class.known(db))
    }

    async fn diagnostic<T>(&self, action: impl FnOnce() -> T) -> Result<T, Infallible> {
        Ok(action())
    }
}

pub(in crate::types::infer::builder::local) async fn class_metadata_with<
    'db,
    E: ClassMetadataEffects<'db>,
>(
    builder: &TypeInferenceBuilder<'db, '_>,
    data: &CallData<'db, '_>,
    class: ClassType<'db>,
    effects: &E,
) -> Result<(), E::Error> {
    let db = builder.db();
    let callable_type = data.callable_type;
    let call_expression = data.call;
    // It might look odd here that we emit an error for class-literals and generic aliases but not
    // `type[]` types. But it's deliberate! The typing spec explicitly mandates that `type[]` types
    // can be called even though class-literals cannot. This is because even though a protocol class
    // `SomeProtocol` is always an abstract class, `type[SomeProtocol]` can be a concrete subclass of
    // that protocol -- and indeed, according to the spec, type checkers must disallow abstract
    // subclasses of the protocol to be passed to parameters that accept `type[SomeProtocol]`.
    // <https://typing.python.org/en/latest/spec/protocol.html#type-and-class-objects-vs-protocols>.
    if effects
        .decision(1, || !callable_type.is_subclass_of())
        .await?
    {
        if let Some(protocol) = effects.protocol_class(db, class).await? {
            effects
                .diagnostic(|| {
                    report_attempted_protocol_instantiation(
                        &builder.context,
                        call_expression,
                        protocol,
                    )
                })
                .await?;
        } else if effects
            .lint_enabled(builder, &CALL_NON_CALLABLE)
            .await?
        {
            let abstract_methods = effects.abstract_methods(db, class).await?;
            if effects.decision(1, || !abstract_methods.is_empty()).await? {
                effects
                    .diagnostic(|| {
                        report_attempted_instantiation_of_abstract_class(
                            &builder.context,
                            call_expression,
                            class,
                            &abstract_methods,
                        )
                    })
                    .await?;
            }
        }
    }

    // Inference of correctly-placed `TypeVar`, `ParamSpec`, `NewType`, and
    // `TypeAliasType` definitions is done in `infer_legacy_typevar`,
    // `infer_paramspec`, `infer_newtype_expression`, and
    // `infer_typealiastype_call`, and doesn't use the full call-binding
    // machinery. If we reach here, it means that someone is trying to
    // instantiate one of these in an invalid context.
    match effects.known(db, class).await? {
        Some(KnownClass::TypeVar | KnownClass::ExtensionsTypeVar) => {
            effects
                .diagnostic(|| {
                    if let Some(builder) = builder
                        .context
                        .report_lint(&INVALID_LEGACY_TYPE_VARIABLE, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `TypeVar` definition must be a simple variable assignment",
                        );
                    }
                })
                .await?
        }
        Some(KnownClass::ParamSpec | KnownClass::ExtensionsParamSpec) => {
            effects
                .diagnostic(|| {
                    if let Some(builder) = builder
                        .context
                        .report_lint(&INVALID_PARAMSPEC, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `ParamSpec` definition must be a simple variable assignment",
                        );
                    }
                })
                .await?
        }
        Some(KnownClass::TypeVarTuple | KnownClass::ExtensionsTypeVarTuple) => {
            effects
                .diagnostic(|| {
                    if let Some(builder) = builder
                        .context
                        .report_lint(&INVALID_LEGACY_TYPE_VARIABLE, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `TypeVarTuple` definition must be a simple variable assignment",
                        );
                    }
                })
                .await?
        }
        Some(KnownClass::NewType) => {
            effects
                .diagnostic(|| {
                    if let Some(builder) = builder
                        .context
                        .report_lint(&INVALID_NEWTYPE, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `NewType` definition must be a simple variable assignment",
                        );
                    }
                })
                .await?
        }
        Some(KnownClass::TypeAliasType | KnownClass::ExtensionsTypeAliasType) => {
            effects
                .diagnostic(|| {
                    if let Some(builder) = builder
                        .context
                        .report_lint(&INVALID_TYPE_ALIAS_TYPE, call_expression)
                    {
                        builder.into_diagnostic(
                            "A `TypeAliasType` definition must be a simple variable assignment",
                        );
                    }
                })
                .await?
        }
        _ => {}
    }
    Ok(())
}
