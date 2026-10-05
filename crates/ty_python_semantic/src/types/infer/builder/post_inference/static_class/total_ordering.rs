//! Validation of the ordering method required by `@total_ordering`.

use std::convert::Infallible;

use ruff_python_ast as ast;

use super::disjoint_decorator::next_class_decorator;
use crate::types::context::InferContext;
use crate::types::diagnostic::report_invalid_total_ordering;
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::{ClassLiteral, StaticClassLiteral, Type};

pub(super) struct OrdinaryTotalOrderingEffects<'a, 'db, 'ast, F> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) file_expression_type: &'a F,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTotalOrderingEffects)]
    pub(in crate::types::infer::builder) trait TotalOrderingEffects<'db> {
        type Error;

        #[operation(local)]
        async fn enabled(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

        #[operation(child)]
        async fn has_ordering_method(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;

        #[operation(local)]
        #[progress]
        async fn next_decorator<'node>(
            &self,
            node: &'node ast::StmtClassDef,
            cursor: &mut usize,
        ) -> Result<Option<&'node ast::Decorator>, Self::Error>;

        #[operation(child)]
        async fn expression_type(&self, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;

        #[operation(source)]
        async fn is_total_ordering(&self, function: FunctionType<'db>) -> Result<bool, Self::Error>;

        #[operation(child)]
        async fn report_missing_method(
            &self,
            class: StaticClassLiteral<'db>,
            decorator: &ast::Decorator,
        ) -> Result<(), Self::Error>;
    }

    #[synchronous(check_total_ordering_sync)]
    #[capabilities(effects = TotalOrderingEffects)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_total_ordering_with<'db, E: TotalOrderingEffects<'db>>(
        class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        effects: &E,
    ) -> Result<(), E::Error> {
        // Check that `@total_ordering` has a valid ordering method in the MRO.
        if !effects.enabled(class).await? || effects.has_ordering_method(class).await? {
            return Ok(());
        }

        // Find the `@total_ordering` decorator to report the diagnostic at its location.
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(decorator) = effects.next_decorator(node, &mut cursor).await? {
            let Type::FunctionLiteral(function) = effects.expression_type(&decorator.expression).await? else {
                continue;
            };
            if effects.is_total_ordering(function).await? {
                return effects.report_missing_method(class, decorator).await;
            }
        }
        Ok(())
    }
}

impl<'db, F: Fn(&ast::Expr) -> Type<'db>> SynchronousTotalOrderingEffects<'db>
    for OrdinaryTotalOrderingEffects<'_, 'db, '_, F>
{
    type Error = Infallible;

    fn enabled(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.total_ordering(self.context.db()))
    }

    fn has_ordering_method(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(class.has_ordering_method_in_mro(self.context.db(), None))
    }

    fn next_decorator<'node>(
        &self,
        node: &'node ast::StmtClassDef,
        cursor: &mut usize,
    ) -> Result<Option<&'node ast::Decorator>, Self::Error> {
        Ok(next_class_decorator(node, cursor))
    }

    fn expression_type(&self, expression: &ast::Expr) -> Result<Type<'db>, Self::Error> {
        Ok((self.file_expression_type)(expression))
    }

    fn is_total_ordering(&self, function: FunctionType<'db>) -> Result<bool, Self::Error> {
        Ok(function.is_known(self.context.db(), KnownFunction::TotalOrdering))
    }

    fn report_missing_method(
        &self,
        class: StaticClassLiteral<'db>,
        decorator: &ast::Decorator,
    ) -> Result<(), Self::Error> {
        report_invalid_total_ordering(self.context, ClassLiteral::Static(class), decorator);
        Ok(())
    }
}
