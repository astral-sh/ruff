//! Base-check state keeps source expressions borrowed from the retained inference builder.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::types::class::base_entries::{
    ClassBaseEntryFacts, expanded_class_base_entries_async_with,
};
use crate::types::class::{CodeGeneratorKind, DisjointBase, ExpandedClassBaseEntry};
use crate::types::context::InferContext;
use crate::types::diagnostic::{
    INVALID_GENERIC_CLASS, IncompatibleBases, report_missing_type_arguments_with,
};
use crate::types::infer::builder::post_inference::static_class::base_checks::{
    ExplicitBaseCheckEffects, next_class_source_base, next_expanded_base_entry,
};
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
use crate::types::subclass_of::SubclassConstructionEffects;
use crate::types::{ClassType, GenericAlias, GenericContext, StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    async fn unavailable_base_check<T>(&self) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::ExplicitBases,
            ))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ExplicitBaseCheckEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn empty_disjoint_bases(&self) -> RunResult<IncompatibleBases<'db>> {
        // Insertion refuses before mutation. This admission therefore covers disposal of
        // the empty map on every exit, including a later MRO refusal or cancellation.
        self.source.local(4, 0, IncompatibleBases::default).await
    }

    async fn empty_typed_dict_bases(&self) -> RunResult<Vec<ClassType<'db>>> {
        // The base-kind effect refuses before appending, so this owner stays allocation-free.
        self.source.local(2, 0, Vec::new).await
    }

    async fn class_definition(&self, node: &ast::StmtClassDef) -> RunResult<Definition<'db>> {
        self.source
            .local(3, 0, || self.builder.index.expect_single_definition(node))
            .await
    }

    async fn explicit_variance_enabled(&self) -> RunResult<bool> {
        self.source
            .is_lint_enabled_source(self.builder, &INVALID_GENERIC_CLASS)
            .await
    }

    async fn next_entry<'node>(
        &self,
        entries: &[ExpandedClassBaseEntry<'node, 'db>],
        cursor: &mut usize,
    ) -> RunResult<Option<(usize, ExpandedClassBaseEntry<'node, 'db>)>> {
        self.source
            .local(1, 0, || next_expanded_base_entry(entries, cursor))
            .await
    }

    async fn next_source_base<'node>(
        &self,
        node: &'node ast::StmtClassDef,
        cursor: &mut usize,
    ) -> RunResult<Option<&'node ast::Expr>> {
        self.source
            .local(1, 0, || next_class_source_base(node, cursor))
            .await
    }

    async fn expand<'node>(
        &self,
        class: StaticClassLiteral<'db>,
        node: &'node ast::StmtClassDef,
        definition: Definition<'db>,
    ) -> RunResult<Vec<ExpandedClassBaseEntry<'node, 'db>>> {
        let file = self.source.static_class_file(class).await?;
        self.source.check_file_program(file).await?;
        let definition_file = self.source.definition_file(definition).await?;
        self.source.check_file_program(definition_file).await?;
        let known = self
            .source
            .field(
                class
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .known(),
            )
            .await?;
        expanded_class_base_entries_async_with(
            known,
            node,
            definition,
            ClassBaseEntryFacts,
            self.source,
        )
        .await
    }

    async fn report_missing_arguments(&self, ty: Type<'db>, node: &ast::Expr) -> RunResult<()> {
        // Use the same missing-argument checks as controlled type annotations, including refusals.
        // Five operations obtain and bind the context, three each obtain and bind the other
        // arguments, and two call the shared checker and forward its future. The tuple pair
        // covers evaluated arguments and callee bindings; the helper covers fixed transfers.
        let bytes = size_of::<[
            (
                &InferContext<'db, '_>,
                Type<'db>,
                &ast::Expr,
                &SourceEffects<'_, 'run, 'db, A>,
            );
            2
        ]>();
        self.source
            .boxed_future_with_fixed_transfers(Ok((16, bytes)), || {
                report_missing_type_arguments_with(&self.builder.context, ty, node, self.source)
            })
            .await?
            .await
    }

    async fn report_named_tuple(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn report_plain_generic(&self, _node: &ast::Expr) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn report_protocol_and_generic(
        &self,
        _node: &ast::Expr,
        _previous: GenericContext<'db>,
        _new: GenericContext<'db>,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn report_protocol_and_type_params(
        &self,
        _node: &ast::Expr,
        _parameters: &ast::TypeParams,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn check_variance(
        &self,
        _class: StaticClassLiteral<'db>,
        _base: GenericAlias<'db>,
        _node: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn nearest_disjoint_base(
        &self,
        _class: ClassType<'db>,
    ) -> RunResult<Option<DisjointBase<'db>>> {
        self.unavailable_base_check().await
    }

    async fn record_disjoint_base(
        &self,
        _bases: &mut IncompatibleBases<'db>,
        _base: DisjointBase<'db>,
        _index: usize,
        _class: ClassType<'db>,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn check_base_kind(
        &self,
        _class: StaticClassLiteral<'db>,
        _base: ClassType<'db>,
        _node: &ast::Expr,
        _is_protocol: bool,
        _kind: Option<CodeGeneratorKind<'db>>,
        _typed_dict_bases: &mut Vec<ClassType<'db>>,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn is_final(&self, class: ClassType<'db>) -> RunResult<bool> {
        SubclassConstructionEffects::is_final(self.source, class).await
    }

    async fn report_final(
        &self,
        _class: StaticClassLiteral<'db>,
        _base: ClassType<'db>,
        _node: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        self.source.work(3).await?;
        Ok(self
            .source
            .static_class_identity(class)
            .await?
            .map(|(class, _)| class))
    }

    async fn is_frozen_dataclass(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<bool>> {
        self.unavailable_base_check().await
    }

    async fn report_frozen(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::StmtClassDef,
        _base: StaticClassLiteral<'db>,
        _source_node: &ast::Expr,
        _base_is_frozen: bool,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn ordered_dataclass_base(
        &self,
        _class: ClassType<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.unavailable_base_check().await
    }

    async fn has_own_comparison_methods(&self, _class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.unavailable_base_check().await
    }

    async fn report_ordered(
        &self,
        _class: StaticClassLiteral<'db>,
        _base: ClassType<'db>,
        _node: &ast::Expr,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }

    async fn expression_type(
        &self,
        definition: Definition<'db>,
        node: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        self.source
            .definition_expression_type(definition, node)
            .await
    }

    async fn is_variable_length_tuple(&self, _ty: Type<'db>) -> RunResult<bool> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::ExplicitBaseTuple,
            ))
            .await
    }

    async fn report_unsupported(
        &self,
        _class: StaticClassLiteral<'db>,
        _node: &ast::Expr,
        _ty: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable_base_check().await
    }
}
