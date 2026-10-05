//! Controlled assignment validation uses the shared decisions and refuses unavailable children.

use ruff_python_ast::AnyNodeRef;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::AssignmentValidationOperation;
use crate::types::Type;
use crate::types::diagnostic::UNSOUND_ASSIGNMENT;
use crate::types::infer::TypeInferenceBuilder;
use crate::types::infer::builder::assignment_validation::{
    AssignmentValidationEffects, AssignmentValidationFacts, AssignmentValidationRequest,
    validate_assignment_with,
};
use crate::types::relation::source::assignability_condition;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Admits the assignment request and its continuation before running shared validation.
    pub(super) async fn validate_assignment_source(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        node: AnyNodeRef<'_>,
        binding: Definition<'db>,
        declaration: Option<Definition<'db>>,
        target: Type<'db>,
        value: Type<'db>,
    ) -> RunResult<bool> {
        let request = self
            .local(
                1,
                size_of::<AssignmentValidationRequest<'_, 'db>>() * 3,
                || AssignmentValidationRequest {
                    node,
                    binding,
                    declaration,
                    target,
                    value,
                },
            )
            .await?;
        self.allocate_future(|| {
            validate_assignment_with(builder, request, AssignmentValidationFacts, self)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> AssignmentValidationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // Prepay the fixed predicates and result retained by the short-circuit decisions.
        self.local(8, size_of::<bool>() * 8, || ()).await
    }

    async fn assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        value: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<bool> {
        self.allocate_future(|| {
            assignability_condition(
                self.db(),
                builder.program_environment(),
                value,
                target,
                self,
            )
        })
        .await?
        .await
    }

    async fn invalid_assignment(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _request: AssignmentValidationRequest<'_, 'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AssignmentValidation(
            AssignmentValidationOperation::InvalidDiagnostic,
        ))
        .await
    }

    async fn unsound_lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<bool> {
        self.allocate_future(|| self.is_lint_enabled_source(builder, &UNSOUND_ASSIGNMENT))
            .await?
            .await
    }

    async fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.file_is_stub(builder.file()).await
    }

    async fn fully_static(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _target: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::AssignmentValidation(
            AssignmentValidationOperation::FullyStatic,
        ))
        .await
    }

    async fn dataclass_body(&self, _builder: &TypeInferenceBuilder<'db, 'ast>) -> RunResult<bool> {
        self.unavailable(SourceOperation::AssignmentValidation(
            AssignmentValidationOperation::DataclassBody,
        ))
        .await
    }

    async fn pure_redundancy(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _value: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::AssignmentValidation(
            AssignmentValidationOperation::PureRedundancy,
        ))
        .await
    }

    async fn unsound_assignment(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _request: AssignmentValidationRequest<'_, 'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::AssignmentValidation(
            AssignmentValidationOperation::UnsoundDiagnostic,
        ))
        .await
    }
}
