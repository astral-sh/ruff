//! Runtime visibility consumes prepared structure and canonical definition inference.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::SemanticIndex;
use ty_python_core::definition::{AssignmentDefinitionKind, Definition, DefinitionKind};
use ty_python_core::scope::FileScopeId;

use super::{PreparedSource, SourceAccess, SourceEffects, SourceOperation};
use crate::NameKind;
use crate::analysis::ImplicitNameOperation;
use crate::types::Type;
use crate::types::function::FunctionDecorators;
use crate::types::infer::DefinitionInference;
use crate::types::runtime_visibility::{RuntimeVisibilityEffects, runtime_visibility_with};
use crate::types::typevar::TypeVarInstance;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn type_checking_range_work(
        &self,
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> RunResult<usize> {
        let mut ancestors = self.local(2, 0, || index.ancestor_scopes(scope)).await?;
        let mut work = 8usize;
        while let Some(next_work) = self
            .local(4, 0, || {
                let Some((scope, _)) = ancestors.next() else {
                    return Ok(None);
                };
                let ranges = index
                    .use_def_map(scope)
                    .range_reachability()
                    .size_hint()
                    .1
                    .ok_or(RunError::Contract(
                        "runtime-visibility range count has no upper bound",
                    ))?;
                Self::checked(
                    ranges
                        .checked_add(1)
                        .and_then(|n| n.checked_mul(4))
                        .and_then(|n| work.checked_add(n)),
                )
                .map(Some)
            })
            .await??
        {
            work = next_work;
        }
        Ok(work)
    }

    pub(in crate::types::infer) async fn infer_runtime_visibility(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        runtime_visibility_with(definition, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RuntimeVisibilityEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Source = PreparedSource<'db>;

    async fn prepare(&self, definition: Definition<'db>) -> RunResult<Self::Source> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let source = self.access.prepare_existing(file).await?;
        self.check_file_program(source.file).await?;
        if source.file != file {
            return Err(RunError::Contract(
                "prepared runtime-visibility file is foreign",
            ));
        }
        Ok(source)
    }

    async fn in_type_checking_block(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
    ) -> RunResult<bool> {
        let db = self.db();
        let definition_scope = self.definition_scope(definition).await?;
        let file = self.scope_file(definition_scope).await?;
        self.check_file_program(file).await?;
        let scope = self
            .field(definition_scope.read_fields(db).file_scope_id())
            .await?;
        // Admit each ancestor step before collecting its range-scan bound.
        // Keep the prepared source alive while the range-counting and range-scanning callbacks
        // borrow its index and parsed module.
        let work = self.type_checking_range_work(&source.index, scope).await?;
        let kind = self.field(definition.read_fields(db).kind()).await?;
        self.local(work, 0, || {
            source
                .index
                .is_in_type_checking_block(scope, kind.full_range(&source.module))
        })
        .await
    }

    async fn is_stub(&self, source: &Self::Source) -> RunResult<bool> {
        self.file_is_stub(self.physical_file(source.file).await?)
            .await
    }

    async fn is_binding(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
        is_stub: bool,
    ) -> RunResult<bool> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        let kind = self.field(definition.read_fields(self.db()).kind()).await?;
        self.local(8, 0, || kind.category(is_stub, &source.module).is_binding())
            .await
    }

    async fn definition_inference(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionInference<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        self.access.definition(definition).await
    }

    async fn binding_type(
        &self,
        inference: &DefinitionInference<'db>,
        definition: Definition<'db>,
    ) -> RunResult<Type<'db>> {
        self.inferred_binding_type(inference, definition).await
    }

    async fn undecorated_type(
        &self,
        inference: &DefinitionInference<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(1, 0, || inference.undecorated_type()).await
    }

    async fn is_type_check_only(&self, ty: Type<'db>) -> RunResult<bool> {
        let db = self.db();
        match ty {
            Type::ClassLiteral(class) => {
                let file = self.class_file(class).await?;
                self.check_file_program(file).await?;
            }
            Type::FunctionLiteral(function) => {
                let file = self.function_file(function).await?;
                self.check_file_program(file).await?;
            }
            _ => {}
        }
        let Type::FunctionLiteral(function) = ty else {
            self.work(3).await?;
            let Some(class) = ty
                .as_class_literal()
                .and_then(crate::types::ClassLiteral::as_static)
            else {
                return Ok(false);
            };
            return self.field(class.field_requests(db).type_check_only()).await;
        };
        self.work(3).await?;
        let literal = self.field(function.field_requests(db).literal()).await?;
        let (overloads, implementation) = literal
            .overloads_and_implementation_with(db, self)
            .await?;
        let work = Self::checked(
            overloads
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(3)),
        )?;
        self.work(work).await?;
        for overload in overloads.iter().copied().chain(implementation) {
            if self
                .field(overload.field_requests(db).decorators())
                .await?
                .contains(FunctionDecorators::TYPE_CHECK_ONLY)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn is_private(
        &self,
        definition: Definition<'db>,
        source: &Self::Source,
    ) -> RunResult<bool> {
        let db = self.db();
        let scope = self.definition_scope(definition).await?;
        let file = self.scope_file(scope).await?;
        self.check_file_program(file).await?;
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        let place = self
            .field(definition.read_fields(db).place_info())
            .await?
            .place();
        let name = self
            .local(5, 0, || {
                place
                    .as_symbol()
                    .map(|symbol| source.index.place_table(file_scope).symbol(symbol).name())
            })
            .await?;
        let Some(name) = name else {
            return Ok(false);
        };
        self.local(Self::checked(name.len().checked_add(1))?, 0, || {
            matches!(NameKind::classify(name), NameKind::Sunder)
        })
        .await
    }

    async fn is_original_typevar(
        &self,
        typevar: TypeVarInstance<'db>,
        definition: Definition<'db>,
    ) -> RunResult<bool> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        self.local(3, 0, || typevar.definition(self.db()) == Some(definition))
            .await
    }

    async fn definition_kind(
        &self,
        definition: Definition<'db>,
    ) -> RunResult<&'db DefinitionKind<'db>> {
        let file = self.definition_file(definition).await?;
        self.check_file_program(file).await?;
        self.field(definition.read_fields(self.db()).kind()).await
    }

    async fn is_type_alias_annotation(
        &self,
        _definition: Definition<'db>,
        _source: &Self::Source,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ImplicitName(
            ImplicitNameOperation::RuntimeVisibilityAlias,
        ))
        .await
    }

    async fn assignment_value<'source>(
        &self,
        assignment: &AssignmentDefinitionKind<'db>,
        source: &'source Self::Source,
    ) -> RunResult<&'source ast::Expr> {
        self.local(1, 0, || assignment.value(&source.module)).await
    }

    async fn subscript_value_type(
        &self,
        _value: &ast::Expr,
        _source: &Self::Source,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ImplicitName(
            ImplicitNameOperation::RuntimeVisibilitySubscript,
        ))
        .await
    }
}
