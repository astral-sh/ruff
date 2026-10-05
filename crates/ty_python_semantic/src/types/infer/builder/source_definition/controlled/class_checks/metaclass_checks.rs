//! Metaclass validation uses the shared selection driver after checking program ownership.

use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::types::class::MetaclassErrorKind;
use crate::types::class::metaclass_selection::static_try_metaclass_with;
use crate::types::infer::builder::post_inference::static_class::metaclass_checks::MetaclassCheckEffects;
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceOperation};
use crate::types::{ClassBase, ClassType, MetaclassCandidate, StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    async fn unavailable_metaclass_check<T>(&self) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Metaclass))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MetaclassCheckEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn metaclass_error(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<MetaclassErrorKind<'db>>> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        let result = static_try_metaclass_with(class, self.source).await?;
        self.source
            .local(2, 0, || result.err().map(|error| error.reason().clone()))
            .await
    }

    async fn invalid_metaclass_range(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
    ) -> RunResult<TextRange> {
        // Keyword lookup needs a resumable scan before diagnostic reporting can be admitted.
        self.unavailable_metaclass_check().await
    }

    async fn report_cycle(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
    ) -> RunResult<()> {
        self.unavailable_metaclass_check().await
    }

    async fn report_generic(&self, _range: TextRange) -> RunResult<()> {
        self.unavailable_metaclass_check().await
    }

    async fn report_not_callable(&self, _range: TextRange, _ty: Type<'db>) -> RunResult<()> {
        self.unavailable_metaclass_check().await
    }

    async fn report_partly_not_callable(&self, _range: TextRange, _ty: Type<'db>) -> RunResult<()> {
        self.unavailable_metaclass_check().await
    }

    async fn report_conflict(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
        _candidate: MetaclassCandidate<'db>,
        _base_metaclass: ClassType<'db>,
        _base: ClassBase<'db>,
    ) -> RunResult<()> {
        self.unavailable_metaclass_check().await
    }
}
