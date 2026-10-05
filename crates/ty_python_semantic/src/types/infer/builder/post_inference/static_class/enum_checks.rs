//! Generic enum validation preserves inheritance checking before generic-context lookup.

use std::convert::Infallible;

use ruff_python_ast as ast;

use crate::types::StaticClassLiteral;
use crate::types::context::InferContext;
use crate::types::diagnostic::INVALID_GENERIC_ENUM;
use crate::types::enums::is_enum_class_by_inheritance;

pub(super) struct OrdinaryGenericEnumEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousGenericEnumEffects)]
    pub(in crate::types::infer::builder) trait GenericEnumEffects<'db> {
        type Error;

        #[operation(child)]
        async fn is_enum(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn has_generic_context(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn report_generic_enum(&self, class: StaticClassLiteral<'db>, node: &ast::StmtClassDef) -> Result<(), Self::Error>;
    }

    #[synchronous(check_generic_enum_sync)]
    #[capabilities(effects = GenericEnumEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_generic_enum_with<'db, E: GenericEnumEffects<'db>>(
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        effects: &E,
    ) -> Result<(), E::Error> {
        if effects.is_enum(class).await? && effects.has_generic_context(class).await? {
            effects.report_generic_enum(class, node).await?;
        }
        Ok(())
    }
}

impl<'db> SynchronousGenericEnumEffects<'db> for OrdinaryGenericEnumEffects<'_, 'db, '_> {
    type Error = Infallible;

    fn is_enum(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(is_enum_class_by_inheritance(
            self.context.db(),
            self.context.program_environment(),
            class,
        ))
    }

    fn has_generic_context(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.generic_context(self.context.db()).is_some())
    }

    fn report_generic_enum(
        &self,
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
    ) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&INVALID_GENERIC_ENUM, node) {
            builder.into_diagnostic(format_args!(
                "Enum class `{}` cannot be generic",
                class.name(self.context.db())
            ));
        }
        Ok(())
    }
}
