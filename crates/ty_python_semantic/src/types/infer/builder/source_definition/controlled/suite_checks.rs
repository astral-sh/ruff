//! Admitted decisions for unused awaitables and redundant conditions in completed suites.

use ruff_db::diagnostic::Severity;
use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::SemanticIndex;
use ty_python_core::definition::Definition;
use ty_python_core::scope::FileScopeId;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::lint::{LintId, LintMetadata, LintSource};
use crate::types::context::{InferContext, LintEligibilityEffects, in_no_type_check_with};
use crate::types::function::{FunctionDecorators, FunctionType};
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::awaitable::{
    AwaitableEffects, check_unused_awaitable_with, is_awaitable_with,
};
use crate::types::infer::builder::redundant_conditions::{
    RedundantConditionEffects, check_suite_for_redundant_conditions_with,
    should_check_redundant_conditions_with,
};
use crate::types::{IntersectionType, NominalInstanceType, Type, UnionType};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer::builder) async fn check_unused_awaitable_source(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> RunResult<()> {
        check_unused_awaitable_with(builder, expression, self).await
    }

    pub(in crate::types::infer::builder) async fn check_redundant_conditions_source(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        suite: &[ast::Stmt],
    ) -> RunResult<()> {
        check_suite_for_redundant_conditions_with(builder, suite, self).await
    }

    pub(in crate::types::infer::builder) async fn is_lint_enabled_source(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
    ) -> RunResult<bool> {
        self.check_file_program(builder.program_file()).await?;
        builder
            .context
            .is_lint_enabled_with(
                lint,
                &SuiteLintEffects {
                    source: self,
                    index: builder.index,
                },
            )
            .await
    }

    /// Returns lint policy metadata using the same controlled eligibility as suite checks.
    pub(in crate::types::infer::builder) async fn lint_severity_source(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: LintId,
    ) -> RunResult<Option<(Severity, LintSource)>> {
        self.check_file_program(builder.program_file()).await?;
        builder.context.lint_severity_with(
            lint,
            &SuiteLintEffects {
                source: self,
                index: builder.index,
            },
        ).await
    }
}

struct SuiteLintEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    index: &'db SemanticIndex<'db>,
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> LintEligibilityEffects<'db, 'ast>
    for SuiteLintEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn diagnostics_suppressed(&self, context: &InferContext<'db, 'ast>) -> RunResult<bool> {
        self.source
            .local(1, 0, || context.diagnostics_suppressed())
            .await
    }

    async fn should_check_file(&self, context: &InferContext<'db, 'ast>) -> RunResult<bool> {
        self.source.access.should_check_file(context.file()).await
    }

    async fn configured_lint(
        &self,
        context: &InferContext<'db, 'ast>,
        lint: LintId,
    ) -> RunResult<Option<(Severity, LintSource)>> {
        let rules = self.source.access.rule_selection(context.file()).await?;
        let units = SourceEffects::<A>::checked(
            rules
                .iter()
                .len()
                .checked_mul(4)
                .and_then(|units| units.checked_add(4)),
        )?;
        self.source.local(units, 0, || rules.get(lint)).await
    }

    async fn no_type_check_flag(&self, context: &InferContext<'db, 'ast>) -> RunResult<bool> {
        self.source
            .local(1, 0, || context.no_type_check_flag())
            .await
    }

    async fn first_scope(&self, context: &InferContext<'db, 'ast>) -> RunResult<FileScopeId> {
        self.source
            .field(context.scope().read_fields(context.db()).file_scope_id())
            .await
    }

    async fn next_ancestor(
        &self,
        _context: &InferContext<'db, 'ast>,
        cursor: &mut Option<FileScopeId>,
    ) -> RunResult<Option<Option<Definition<'db>>>> {
        // Each definition lookup searches at most two sorted maps with fixed-size node keys.
        let units = 2 * usize::BITS as usize + 8;
        self.source
            .local(units, 0, || {
                InferContext::next_lint_ancestor(self.index, cursor)
            })
            .await
    }

    async fn undecorated_function(
        &self,
        _context: &InferContext<'db, 'ast>,
        definition: Definition<'db>,
    ) -> RunResult<Option<FunctionType<'db>>> {
        let file = self.source.definition_file(definition).await?;
        self.source.check_file_program(file).await?;
        let inference = self.source.access.definition(definition).await?;
        self.source
            .local(1, 0, || {
                inference
                    .undecorated_type()
                    .and_then(Type::as_function_literal)
            })
            .await
    }

    async fn function_has_no_type_check(
        &self,
        context: &InferContext<'db, 'ast>,
        function: FunctionType<'db>,
    ) -> RunResult<bool> {
        let (overloads, implementation) = function
            .overloads_and_implementation_with(context.db(), self.source)
            .await?;
        let mut definitions = overloads.iter().copied().chain(implementation);
        loop {
            let next = self.source.local(1, 0, || definitions.next()).await?;
            let Some(definition) = next else {
                return Ok(false);
            };
            let fields = self.source.access.endpoint().field_request_context();
            let decorators = self
                .source
                .field(definition.field_requests(fields).decorators())
                .await?;
            if self
                .source
                .local(1, 0, || {
                    decorators.contains(FunctionDecorators::NO_TYPE_CHECK)
                })
                .await?
            {
                return Ok(true);
            }
        }
    }

    async fn in_no_type_check(&self, context: &InferContext<'db, 'ast>) -> RunResult<bool> {
        in_no_type_check_with(context, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> AwaitableEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn expression_type(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        let units = Self::checked(
            builder
                .expressions
                .capacity()
                .checked_mul(4)
                .and_then(|units| units.checked_add(4)),
        )?;
        self.local(units, 0, || builder.expression_type(expression))
            .await
    }

    async fn is_awaitable(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        is_awaitable_with(builder, ty, self).await
    }

    async fn nominal_awaitable(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::SuiteAwaitableNominal)
            .await
    }

    async fn union_awaitable(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _union: UnionType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::SuiteAwaitableUnion).await
    }

    async fn intersection_awaitable(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _intersection: IntersectionType<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::SuiteAwaitableIntersection)
            .await
    }

    async fn is_known_function_call(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _expression: &ast::Expr,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::SuiteAwaitableKnownFunction)
            .await
    }

    async fn report_unused_awaitable(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _expression: &ast::Expr,
        _ty: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::SuiteAwaitableDiagnostic)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RedundantConditionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn in_string_annotation(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
    ) -> RunResult<bool> {
        self.local(1, 0, || builder.in_string_annotation()).await
    }

    async fn should_check_file(&self, builder: &TypeInferenceBuilder<'db, '_>) -> RunResult<bool> {
        self.access.should_check_file(builder.file()).await
    }

    async fn is_stub(&self, builder: &TypeInferenceBuilder<'db, '_>) -> RunResult<bool> {
        self.file_is_stub(builder.file()).await
    }

    async fn is_lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        lint: &'static LintMetadata,
    ) -> RunResult<bool> {
        self.is_lint_enabled_source(builder, lint).await
    }

    async fn should_check(&self, builder: &TypeInferenceBuilder<'db, '_>) -> RunResult<bool> {
        should_check_redundant_conditions_with(builder, self).await
    }

    async fn next_statement<'suite>(
        &self,
        suite: &'suite [ast::Stmt],
        cursor: &mut usize,
    ) -> RunResult<Option<(usize, &'suite ast::Stmt)>> {
        self.local(1, 0, || {
            let next = suite.get(*cursor).map(|statement| (*cursor, statement));
            if next.is_some() {
                *cursor += 1;
            }
            next
        })
        .await
    }

    async fn check_if(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _statement: &ast::StmtIf,
        _suite: &[ast::Stmt],
        _index: usize,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::SuiteRedundantIf).await
    }

    async fn check_assert(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _statement: &ast::StmtAssert,
        _suite: &[ast::Stmt],
        _index: usize,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::SuiteRedundantAssert)
            .await
    }

    async fn check_while(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _statement: &ast::StmtWhile,
        _suite: &[ast::Stmt],
        _index: usize,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::SuiteRedundantWhile).await
    }

    async fn check_match(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _statement: &ast::StmtMatch,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::SuiteRedundantMatch).await
    }
}
