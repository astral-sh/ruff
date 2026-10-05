//! Dataclass application checks preserve the priority of incompatible class kinds.

use std::convert::Infallible;

use crate::types::StaticClassLiteral;
use crate::types::context::InferContext;
use crate::types::diagnostic::INVALID_DATACLASS;
use crate::types::enums::is_enum_class_by_inheritance;

#[cfg(test)]
mod tests;

pub(super) struct OrdinaryDataclassApplicationEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDataclassApplicationEffects)]
    pub(in crate::types::infer::builder) trait DataclassApplicationEffects<'db> {
        type Error;

        #[operation(source)]
        async fn has_dataclass_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn has_named_tuple_class_in_mro(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn is_enum(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_named_tuple(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_enum(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn report_protocol(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error>;
    }

    #[synchronous(check_dataclass_application_sync)]
    #[capabilities(effects = DataclassApplicationEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_dataclass_application_with<'db, E: DataclassApplicationEffects<'db>>(
        class: StaticClassLiteral<'db>,
        is_protocol: bool,
        effects: &E,
    ) -> Result<(), E::Error> {
        if effects.has_dataclass_params(class).await? {
            if effects.has_named_tuple_class_in_mro(class).await? {
                effects.report_named_tuple(class).await?;
            } else if effects.is_typed_dict(class).await? {
                effects.report_typed_dict(class).await?;
            } else if effects.is_enum(class).await? {
                effects.report_enum(class).await?;
            } else if is_protocol {
                effects.report_protocol(class).await?;
            }
        }
        Ok(())
    }
}

impl<'db> SynchronousDataclassApplicationEffects<'db>
    for OrdinaryDataclassApplicationEffects<'_, 'db, '_>
{
    type Error = Infallible;

    fn has_dataclass_params(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.dataclass_params(self.context.db()).is_some())
    }

    fn has_named_tuple_class_in_mro(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(class.has_named_tuple_class_in_mro(self.context.db()))
    }

    fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.is_typed_dict(self.context.db()))
    }

    fn is_enum(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(is_enum_class_by_inheritance(
            self.context.db(),
            self.context.program_environment(),
            class,
        ))
    }

    fn report_named_tuple(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "`NamedTuple` class `{}` cannot be decorated with `@dataclass`",
                class.name(db),
            ));
            diagnostic.info("An exception will be raised when instantiating the class at runtime");
        }
        Ok(())
    }

    fn report_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "`TypedDict` class `{}` cannot be decorated with `@dataclass`",
                class.name(db),
            ));
            diagnostic
                .info("An exception will often be raised when instantiating the class at runtime");
        }
        Ok(())
    }

    fn report_enum(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Enum class `{}` cannot be decorated with `@dataclass`",
                class.name(db),
            ));
            diagnostic.info("Applying `@dataclass` to an enum is not supported at runtime");
        }
        Ok(())
    }

    fn report_protocol(&self, class: StaticClassLiteral<'db>) -> Result<(), Self::Error> {
        let context = self.context;
        let db = context.db();
        if let Some(builder) = context.report_lint(&INVALID_DATACLASS, class.header_range(db)) {
            let mut diagnostic = builder.into_diagnostic(format_args!(
                "Protocol class `{}` cannot be decorated with `@dataclass`",
                class.name(db),
            ));
            diagnostic.info("Protocols define abstract interfaces and cannot be instantiated");
        }
        Ok(())
    }
}
