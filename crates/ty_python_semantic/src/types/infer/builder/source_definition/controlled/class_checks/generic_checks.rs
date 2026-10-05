//! Generic validation selects canonical contexts before inspecting their variables.

mod base_shadowing;
mod default_references;
mod legacy_defaults;
mod own_shadowing;
mod parent_scope;
mod parameters;
mod reports;
mod scan_cost;

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::FileScopeId;

use super::ClassCheckEffects;
use crate::FxIndexSet;
use crate::analysis::ClassCheckOperation;
use crate::types::class::base_typevars::typevars_referenced_in_bases_with;
use crate::types::class::context::{
    inherited_legacy_generic_context_with, legacy_generic_context_async_with,
    pep695_generic_context_with,
};
use crate::types::diagnostic::INVALID_GENERIC_CLASS;
use crate::types::infer::builder::post_inference::static_class::generic_checks::{
    ClassGenericCheckEffects, next_class_base_variable,
};
use crate::types::infer::builder::source_definition::controlled::{SourceAccess, SourceOperation};
use crate::types::local_transfer::collections::{CALL_1, CALL_2, checked as checked_quote, event_quote};
use crate::types::local_transfer::context_variables::context_variable_at_quote;
use crate::types::{BoundTypeVarInstance, GenericContext, StaticClassLiteral};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    async fn check_generic_class_program(&self, class: StaticClassLiteral<'db>) -> RunResult<()> {
        let scope = self.source.boxed_future_with_fixed_transfers(
            Ok((10, size_of::<[(StaticClassLiteral<'db>, &Self); 2]>())),
            || self.generic_class_body_scope(class),
        ).await?.await?;
        let file = self.source.boxed_future_with_fixed_transfers(
            Ok((10, size_of::<[(ty_python_core::scope::ScopeId<'db>, &Self); 2]>())),
            || self.generic_scope_file(scope),
        ).await?.await?;
        self.source.boxed_future_with_fixed_transfers(
            Ok((10, size_of::<[(ty_python_core::ProgramFile<'db>, &Self); 2]>())),
            || self.check_generic_file_program(file),
        ).await?.await
    }

    async fn unavailable_generic_check<T>(&self) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::GenericContext,
            ))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassGenericCheckEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn pep695_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.check_generic_class_program(class).await?;
        pep695_generic_context_with(class, self.source).await
    }

    async fn inherited_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.check_generic_class_program(class).await?;
        inherited_legacy_generic_context_with(class, self.source).await
    }

    async fn legacy_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.check_generic_class_program(class).await?;
        legacy_generic_context_async_with(class, self.source).await
    }

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.check_generic_class_program(class).await?;
        self.source.access.class_generic_context(class).await
    }

    async fn check_inherited_variables(
        &self,
        _class_node: &ast::StmtClassDef,
        _context: GenericContext<'db>,
    ) -> RunResult<()> {
        self.unavailable_generic_check().await
    }

    async fn check_inherited_subset(
        &self,
        _class_node: &ast::StmtClassDef,
        _legacy: GenericContext<'db>,
        _inherited: GenericContext<'db>,
    ) -> RunResult<()> {
        self.unavailable_generic_check().await
    }

    async fn check_type_params(
        &self,
        class_node: &ast::StmtClassDef,
        type_params: &ast::TypeParams,
    ) -> RunResult<()> {
        self.check_parameter_lists(class_node, type_params).await
    }

    async fn invalid_generic_class_enabled(&self) -> RunResult<bool> {
        self.source
            .is_lint_enabled_source(self.builder, &INVALID_GENERIC_CLASS)
            .await
    }

    async fn check_legacy_defaults(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        context: GenericContext<'db>,
    ) -> RunResult<()> {
        self.check_class_legacy_default_order(class, class_node, context).await
    }

    async fn check_default_references(
        &self,
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<()> {
        self.check_class_default_references(class, context).await
    }

    async fn parent_scope(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<FileScopeId>> {
        self.source.boxed_future_with_fixed_transfers(
            Ok((10, size_of::<[(StaticClassLiteral<'db>, &Self); 2]>())),
            || self.generic_class_parent_scope(class),
        ).await?.await
    }

    async fn check_own_shadowing(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        parent: FileScopeId,
        context: GenericContext<'db>,
    ) -> RunResult<()> {
        self.check_class_own_shadowing(class, class_node, parent, context).await
    }

    async fn base_variables(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<FxIndexSet<BoundTypeVarInstance<'db>>> {
        self.source
            .boxed_future_with_fixed_transfers(
                Ok((12, size_of::<[(StaticClassLiteral<'db>, &Self); 2]>())),
                || typevars_referenced_in_bases_with(class, self),
            )
            .await?
            .await
    }

    async fn next_base_variable(
        &self,
        variables: &FxIndexSet<BoundTypeVarInstance<'db>>,
        cursor: &mut usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        // The ordered-map lookup quote bounds IndexSet's shorter entry-slice lookup too.
        // Add copying, cursor advancement and the shared caller's loop/return branches.
        let (work, bytes) = const {
            match (
                context_variable_at_quote(),
                checked_quote(event_quote(3 * CALL_1 + CALL_2 + 26, &[
                    size_of::<(&FxIndexSet<BoundTypeVarInstance<'static>>, &mut usize)>(),
                    size_of::<Option<&BoundTypeVarInstance<'static>>>(),
                    size_of::<Option<BoundTypeVarInstance<'static>>>(),
                    size_of::<(usize, bool)>(),
                ])),
            ) {
                (Ok((work, bytes)), Ok((step_work, step_bytes))) => {
                    Ok((work + step_work, bytes + step_bytes))
                }
                (Err(error), _) | (_, Err(error)) => Err(error),
            }
        }?;
        self.source
            .local_with_fixed_transfers(work, bytes, || next_class_base_variable(variables, cursor))
            .await
    }

    async fn check_base_shadowing(
        &self,
        class: StaticClassLiteral<'db>,
        class_node: &ast::StmtClassDef,
        parent: FileScopeId,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.check_class_base_shadowing(class, class_node, parent, variable).await
    }
}
