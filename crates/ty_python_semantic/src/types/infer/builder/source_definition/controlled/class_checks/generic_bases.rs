//! Generic-base validation retains its accumulator across admitted operations.

use ruff_python_ast as ast;
use ruff_text_size::TextRange;
use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::types::diagnostic::generic_bases::{
    GenericBaseCheckEffects, GenericBaseConstraint, GenericBaseConstraints, next_generic_base_type,
    report_inconsistent_generic_bases_with,
};
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceOperation};
use crate::types::{ClassLiteral, ClassType, GenericAlias, StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    pub(super) async fn check_generic_bases(
        &self,
        header_range: TextRange,
        explicit_bases: &[Type<'db>],
        base_nodes: Option<&[ast::Expr]>,
    ) -> RunResult<bool> {
        report_inconsistent_generic_bases_with(header_range, explicit_bases, base_nodes, self).await
    }

    async fn unavailable_generic_base_check<T>(&self) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Mro))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> GenericBaseCheckEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;
    type Ancestors<'state>
        = ()
    where
        Self: 'state;

    async fn empty_constraints(&self) -> RunResult<GenericBaseConstraints<'db>> {
        // Lookup and insertion refuse before mutation, so this admission covers construction
        // and disposal of the allocation-free map on every exit.
        self.source
            .local(4, 0, GenericBaseConstraints::default)
            .await
    }

    async fn next_type(
        &self,
        types: &[Type<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<(usize, Type<'db>)>> {
        self.source
            .local(1, 0, || next_generic_base_type(types, cursor))
            .await
    }

    async fn has_generic_context(&self, _class: ClassLiteral<'db>) -> RunResult<bool> {
        self.unavailable_generic_base_check().await
    }

    async fn ancestors_start(&self, _class: ClassType<'db>) -> RunResult<Self::Ancestors<'_>> {
        self.unavailable_generic_base_check().await
    }

    async fn ancestors_next<'state>(
        &'state self,
        _cursor: &mut Self::Ancestors<'state>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.unavailable_generic_base_check().await
    }

    async fn origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        let origin = self
            .source
            .field(
                alias
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .origin(),
            )
            .await?;
        let file = self.source.static_class_file(origin).await?;
        self.source.check_file_program(file).await?;
        Ok(origin)
    }

    async fn arguments(&self, alias: GenericAlias<'db>) -> RunResult<&'db [Type<'db>]> {
        self.origin(alias).await?;
        let context = self.source.access.endpoint().field_request_context();
        let specialization = self
            .source
            .field(alias.field_requests(context).specialization())
            .await?;
        let types = self
            .source
            .field(specialization.field_requests(context).types())
            .await?;
        self.source.local(1, 0, || types.as_ref()).await
    }

    async fn is_dynamic(&self, argument: Type<'db>) -> RunResult<bool> {
        self.source.local(1, 0, || argument.is_dynamic()).await
    }

    async fn remember_argument(
        &self,
        _constraints: &mut GenericBaseConstraints<'db>,
        _origin: StaticClassLiteral<'db>,
        _parameter_index: usize,
        _current: GenericBaseConstraint<'db>,
    ) -> RunResult<GenericBaseConstraint<'db>> {
        self.unavailable_generic_base_check().await
    }

    async fn same_argument(
        &self,
        earlier: GenericBaseConstraint<'db>,
        argument: Type<'db>,
    ) -> RunResult<bool> {
        self.source
            .local(1, 0, || earlier.has_argument(argument))
            .await
    }

    async fn same_base(
        &self,
        earlier: GenericBaseConstraint<'db>,
        base_index: usize,
    ) -> RunResult<bool> {
        self.source
            .local(1, 0, || earlier.has_base(base_index))
            .await
    }

    async fn report_conflict(
        &self,
        _header_range: TextRange,
        _base_nodes: Option<&[ast::Expr]>,
        _base: Type<'db>,
        _origin: StaticClassLiteral<'db>,
        _earlier: GenericBaseConstraint<'db>,
        _later: GenericBaseConstraint<'db>,
    ) -> RunResult<()> {
        self.unavailable_generic_base_check().await
    }
}
