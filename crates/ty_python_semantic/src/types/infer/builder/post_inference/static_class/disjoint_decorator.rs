//! Shared validation of the first recognized `@disjoint_base` decorator.

use std::convert::Infallible;

use ruff_python_ast as ast;

use crate::types::class::CodeGeneratorKind;
use crate::types::context::InferContext;
use crate::types::diagnostic::{INVALID_PROTOCOL, INVALID_TYPED_DICT_HEADER};
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::{StaticClassLiteral, Type};

#[cfg(test)]
mod tests;

pub(super) struct OrdinaryDisjointBaseDecoratorEffects<'a, 'db, 'ast, F> {
    pub(super) context: &'a InferContext<'db, 'ast>,
    pub(super) file_expression_type: &'a F,
}

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousDisjointBaseDecoratorEffects)]
    pub(in crate::types::infer::builder) trait DisjointBaseDecoratorEffects<'db> {
        type Error;

        #[operation(local)]
        #[progress]
        async fn next_decorator<'node>(
            &self,
            class_node: &'node ast::StmtClassDef,
            cursor: &mut usize,
        ) -> Result<Option<&'node ast::Decorator>, Self::Error>;

        #[operation(child)]
        async fn expression_type(&self, expression: &ast::Expr) -> Result<Type<'db>, Self::Error>;

        #[operation(source)]
        async fn is_known_function(
            &self,
            function: FunctionType<'db>,
            known: KnownFunction,
        ) -> Result<bool, Self::Error>;

        #[operation(child)]
        async fn report_typed_dict(
            &self,
            class: StaticClassLiteral<'db>,
            decorator: &ast::Decorator,
        ) -> Result<(), Self::Error>;

        #[operation(child)]
        async fn report_protocol(
            &self,
            class: StaticClassLiteral<'db>,
            decorator: &ast::Decorator,
        ) -> Result<(), Self::Error>;
    }

    #[synchronous(check_disjoint_base_decorator_sync)]
    #[capabilities(effects = DisjointBaseDecoratorEffects)]
    #[passive_values(KnownFunction)]
    pub(in crate::types::infer::builder) async fn check_disjoint_base_decorator_with<'db, E: DisjointBaseDecoratorEffects<'db>>(
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        class_kind: Option<CodeGeneratorKind<'db>>,
        is_protocol: bool,
        effects: &E,
    ) -> Result<(), E::Error> {
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(decorator) = effects.next_decorator(class_node, &mut cursor).await? {
            let Type::FunctionLiteral(function) = effects.expression_type(&decorator.expression).await? else {
                continue;
            };
            if !effects.is_known_function(function, KnownFunction::DisjointBase).await? {
                continue;
            }

            if matches!(class_kind, Some(CodeGeneratorKind::TypedDict)) {
                effects.report_typed_dict(class, decorator).await?;
            } else if is_protocol {
                effects.report_protocol(class, decorator).await?;
            }
            return Ok(());
        }
        Ok(())
    }
}

/// Advances within the borrowed class node without detaching decorators from their AST owner.
/// Controlled callers admit the step before invoking this helper.
pub(in crate::types::infer::builder) fn next_class_decorator<'node>(
    class_node: &'node ast::StmtClassDef,
    cursor: &mut usize,
) -> Option<&'node ast::Decorator> {
    let decorator = class_node.decorator_list.get(*cursor)?;
    *cursor += 1;
    Some(decorator)
}

impl<'db, F: Fn(&ast::Expr) -> Type<'db>> SynchronousDisjointBaseDecoratorEffects<'db>
    for OrdinaryDisjointBaseDecoratorEffects<'_, 'db, '_, F>
{
    type Error = Infallible;

    fn next_decorator<'node>(
        &self,
        class_node: &'node ast::StmtClassDef,
        cursor: &mut usize,
    ) -> Result<Option<&'node ast::Decorator>, Self::Error> {
        Ok(next_class_decorator(class_node, cursor))
    }

    fn expression_type(&self, expression: &ast::Expr) -> Result<Type<'db>, Self::Error> {
        Ok((self.file_expression_type)(expression))
    }

    fn is_known_function(
        &self,
        function: FunctionType<'db>,
        known: KnownFunction,
    ) -> Result<bool, Self::Error> {
        Ok(function.is_known(self.context.db(), known))
    }

    fn report_typed_dict(
        &self,
        class: StaticClassLiteral<'db>,
        decorator: &ast::Decorator,
    ) -> Result<(), Self::Error> {
        if let Some(builder) = self
            .context
            .report_lint(&INVALID_TYPED_DICT_HEADER, decorator)
        {
            builder.into_diagnostic(format_args!(
                "`@disjoint_base` cannot be used with `TypedDict` class `{}`",
                class.name(self.context.db()),
            ));
        }
        Ok(())
    }

    fn report_protocol(
        &self,
        class: StaticClassLiteral<'db>,
        decorator: &ast::Decorator,
    ) -> Result<(), Self::Error> {
        if let Some(builder) = self.context.report_lint(&INVALID_PROTOCOL, decorator) {
            builder.into_diagnostic(format_args!(
                "`@disjoint_base` cannot be used with protocol class `{}`",
                class.name(self.context.db()),
            ));
        }
        Ok(())
    }
}
