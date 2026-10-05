//! Class-header arguments retain their AST owner and admitted call-argument storage.

use ruff_python_ast::{self as ast, PythonVersion};
use salsa::execution_probe::{RunError, RunResult};

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::place::{Place, PlaceAndQualifiers};
use crate::types::call::Argument;
use crate::types::infer::builder::post_inference::static_class::argument_checks::{
    ClassArgumentCheckEffects, next_class_keyword,
};
use crate::types::infer::builder::post_inference::static_class::slot_checks::ClassSlotCheckEffects;
use crate::types::infer::builder::source_definition::controlled::own_member::SourceMemberMroSelection;
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
use crate::types::local_transfer::collections::{CALL_1, CALL_5};
use crate::types::{
    CallArguments, ClassLiteral, ClassType, MemberLookupPolicy, StaticClassLiteral, Type, TypeContext,
    TypingModule,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassCheckEffects<'_, '_, 'run, 'db, '_, A> {
    async fn unavailable_argument_check<T>(&self) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Arguments))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassArgumentCheckEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn in_stub(&self) -> RunResult<bool> {
        ClassSlotCheckEffects::in_stub(self).await
    }

    async fn typed_dict_module(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<TypingModule>> {
        self.unavailable_argument_check().await
    }

    async fn python_version(&self) -> RunResult<PythonVersion> {
        let context = self.source.access.endpoint().field_request_context();
        let environment = self
            .source
            .field(
                self.source
                    .program
                    .field_requests(context)
                    .resolver_environment(),
            )
            .await?;
        self.source
            .field(environment.read_fields(context).python_version())
            .await
    }

    async fn next_keyword<'node>(
        &self,
        arguments: &'node ast::Arguments,
        cursor: &mut usize,
    ) -> RunResult<Option<&'node ast::Keyword>> {
        self.source
            .local(1, 0, || next_class_keyword(arguments, cursor))
            .await
    }

    async fn expression_type(&self, _expression: &ast::Expr) -> RunResult<Type<'db>> {
        self.unavailable_argument_check().await
    }

    async fn report_pep_728_unavailable(
        &self,
        _keyword: &ast::Keyword,
        _argument_name: &str,
    ) -> RunResult<()> {
        self.unavailable_argument_check().await
    }

    async fn report_invalid_boolean(
        &self,
        _keyword: &ast::Keyword,
        _argument_name: &str,
        _passed_type: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable_argument_check().await
    }

    async fn report_custom_metaclass(&self, _keyword: &ast::Keyword) -> RunResult<()> {
        self.unavailable_argument_check().await
    }

    async fn report_unknown_keyword(
        &self,
        _keyword: &ast::Keyword,
        _argument_name: &str,
    ) -> RunResult<()> {
        self.unavailable_argument_check().await
    }

    async fn report_keyword_variadic(&self, _keyword: &ast::Keyword) -> RunResult<()> {
        self.unavailable_argument_check().await
    }

    async fn new_call_arguments<'node>(
        &self,
        arguments: &'node ast::Arguments,
    ) -> RunResult<CallArguments<'node, 'db>> {
        let count = arguments.keywords.len();
        let bytes = SourceEffects::<A>::checked(CallArguments::capacity_bytes(count))?;
        // Each keyword contributes at most one entry with an empty contextual-type map.
        // Prepay destruction of those entries on success, refusal and cancellation.
        let work =
            SourceEffects::<A>::checked(count.checked_mul(2).and_then(|n| n.checked_add(4)))?;
        self.source
            .local(work, bytes, || CallArguments::with_capacity(count))
            .await
    }

    async fn append_argument<'node>(
        &self,
        arguments: &mut CallArguments<'node, 'db>,
        argument: Argument<'node>,
        ty: Type<'db>,
    ) -> RunResult<()> {
        self.source
            .local(4, 0, || {
                let index = arguments.len();
                arguments.push_preallocated_argument(argument)?;
                arguments.insert_type(index, TypeContext::default(), ty);
                Ok(())
            })
            .await?
    }

    async fn check_inherited_init_subclass(
        &self,
        class: StaticClassLiteral<'db>,
        _class_node: &ast::StmtClassDef,
        _call_arguments: CallArguments<'_, 'db>,
    ) -> RunResult<()> {
        let class = self
            .source
            .local_with_fixed_transfers(2, 0, || ClassType::NonGeneric(ClassLiteral::Static(class)))
            .await?;
        let member = self
            .source
            .local_quoted_with_fixed_transfers(
                Ok((
                    CALL_5 + 4,
                    size_of::<[
                        (
                            &SourceEffects<'_, 'run, 'db, A>,
                            ClassType<'db>,
                            &str,
                            MemberLookupPolicy,
                            SourceMemberMroSelection,
                        );
                        2
                    ]>() + size_of::<[RunResult<PlaceAndQualifiers<'db>>; 4]>(),
                )),
                || {
                    self.source.source_class_member_from_mro(
                        class,
                        "__init_subclass__",
                        MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
                        SourceMemberMroSelection::Inherited,
                    )
                },
            )
            .await?
            .await?;
        let missing = self
            .source
            .local_with_fixed_transfers(
                3 * CALL_1 + 4,
                size_of::<[&Place<'db>; 2]>() + size_of::<[Option<Type<'db>>; 2]>(),
                || member.ignore_possibly_undefined().is_none(),
            )
            .await?;
        if missing {
            Ok(())
        } else {
            self.source
                .local_quoted_with_fixed_transfers(
                    Ok((
                        CALL_1 + 4,
                        size_of::<[&Self; 2]>() + size_of::<[RunResult<()>; 4]>(),
                    )),
                    || self.unavailable_argument_check(),
                )
                .await?
                .await
        }
    }
}
