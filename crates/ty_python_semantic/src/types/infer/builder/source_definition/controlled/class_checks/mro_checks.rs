//! MRO validation borrows canonical results and retains the caller's base-check state.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::types::class::ExpandedClassBaseEntry;
use crate::types::class::context::explicit_class_bases_with;
use crate::types::diagnostic::IncompatibleBases;
use crate::types::infer::builder::post_inference::static_class::mro_checks::{
    MroCheckEffects, base_source_nodes, next_duplicate, next_invalid,
};
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
use crate::types::mro::{DuplicateBaseError, StaticMroErrorKind};
use crate::types::{StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    async fn unavailable_mro_check<T>(&self) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Mro))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MroCheckEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn mro_error(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<&'db StaticMroErrorKind<'db>>> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        let result = self.source.access.static_mro(class).await?;
        self.source
            .local(1, 0, || match result {
                Ok(_) => None,
                Err(error) => Some(error.reason()),
            })
            .await
    }

    async fn next_duplicate<'a>(
        &self,
        duplicates: &'a [DuplicateBaseError<'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<&'a DuplicateBaseError<'db>>> {
        self.source
            .local(1, 0, || next_duplicate(duplicates, cursor))
            .await
    }

    async fn report_duplicate(
        &self,
        _class: StaticClassLiteral<'db>,
        _duplicate: &DuplicateBaseError<'db>,
        _entries: &[ExpandedClassBaseEntry<'_, 'db>],
    ) -> RunResult<()> {
        self.unavailable_mro_check().await
    }

    async fn next_invalid(
        &self,
        invalid: &[(usize, Type<'db>)],
        cursor: &mut usize,
    ) -> RunResult<Option<(usize, Type<'db>)>> {
        self.source
            .local(1, 0, || next_invalid(invalid, cursor))
            .await
    }

    async fn report_invalid(
        &self,
        _class: StaticClassLiteral<'db>,
        _index: usize,
        _ty: Type<'db>,
        _entries: &[ExpandedClassBaseEntry<'_, 'db>],
    ) -> RunResult<()> {
        self.unavailable_mro_check().await
    }

    async fn report_unresolvable(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
        _bases: &[Type<'db>],
        _generic_index: &Option<usize>,
    ) -> RunResult<()> {
        self.unavailable_mro_check().await
    }

    async fn report_pep695(&self, _node: &ast::StmtClassDef) -> RunResult<()> {
        self.unavailable_mro_check().await
    }

    async fn report_cycle(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
    ) -> RunResult<()> {
        self.unavailable_mro_check().await
    }

    async fn prune_disjoint(&self, bases: &mut IncompatibleBases<'db>) -> RunResult<()> {
        self.prune_disjoint_bases(bases).await
    }

    async fn has_layout_conflict(&self, bases: &IncompatibleBases<'db>) -> RunResult<bool> {
        Ok(self.disjoint_base_count(bases).await? > 1)
    }

    async fn report_layout(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
        _bases: &IncompatibleBases<'db>,
    ) -> RunResult<()> {
        self.unavailable_mro_check().await
    }

    async fn explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<&'db [Type<'db>]> {
        explicit_class_bases_with(class, self.source).await
    }

    async fn source_nodes<'node>(
        &self,
        node: &'node ast::StmtClassDef,
        bases: &[Type<'db>],
    ) -> RunResult<Option<&'node [ast::Expr]>> {
        let work = SourceEffects::<A>::checked(node.bases().len().checked_add(3))?;
        self.source
            .local(work, 0, || base_source_nodes(node, bases.len()))
            .await
    }

    async fn check_generic(
        &self,
        _class: StaticClassLiteral<'db>,
        node: &ast::StmtClassDef,
        bases: &[Type<'db>],
        source_nodes: Option<&[ast::Expr]>,
    ) -> RunResult<bool> {
        let header_range = self
            .source
            .local(3, 0, || StaticClassLiteral::header_range_from_node(node))
            .await?;
        self.check_generic_bases(header_range, bases, source_nodes)
            .await
    }
}
