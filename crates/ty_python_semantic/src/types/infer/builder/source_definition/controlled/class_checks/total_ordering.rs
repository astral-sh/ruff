use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::infer::builder::post_inference::static_class::disjoint_decorator::DisjointBaseDecoratorEffects;
use crate::types::infer::builder::post_inference::static_class::total_ordering::TotalOrderingEffects;
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceOperation};
use crate::types::{StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TotalOrderingEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn enabled(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        self.source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .total_ordering(),
            )
            .await
    }

    async fn has_ordering_method(&self, _class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::TotalOrdering,
            ))
            .await
    }

    async fn next_decorator<'node>(
        &self,
        node: &'node ast::StmtClassDef,
        cursor: &mut usize,
    ) -> RunResult<Option<&'node ast::Decorator>> {
        DisjointBaseDecoratorEffects::next_decorator(self, node, cursor).await
    }

    async fn expression_type(&self, _expression: &ast::Expr) -> RunResult<Type<'db>> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::TotalOrdering,
            ))
            .await
    }

    async fn is_total_ordering(&self, function: FunctionType<'db>) -> RunResult<bool> {
        DisjointBaseDecoratorEffects::is_known_function(
            self,
            function,
            KnownFunction::TotalOrdering,
        )
        .await
    }

    async fn report_missing_method(
        &self,
        _class: StaticClassLiteral<'db>,
        _decorator: &ast::Decorator,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::TotalOrdering,
            ))
            .await
    }
}
