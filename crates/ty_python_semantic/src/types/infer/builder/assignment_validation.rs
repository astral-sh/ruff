//! Assignment validation preserves assignability and the ordered unsound-assignment checks.

use std::convert::Infallible;

use ruff_python_ast::AnyNodeRef;
use ty_python_core::definition::Definition;

use super::TypeInferenceBuilder;
use crate::types::Type;
use crate::types::diagnostic::{
    UNSOUND_ASSIGNMENT, report_invalid_assignment, report_unsound_assignment,
};

/// Retains the assignment site and types for reports about the original binding and declaration.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct AssignmentValidationRequest<'node, 'db> {
    pub(in crate::types::infer) node: AnyNodeRef<'node>,
    pub(in crate::types::infer) binding: Definition<'db>,
    pub(in crate::types::infer) declaration: Option<Definition<'db>>,
    pub(in crate::types::infer) target: Type<'db>,
    pub(in crate::types::infer) value: Type<'db>,
}

/// Classifies the gradual-target fast path without consulting semantic dependencies.
#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer) struct AssignmentValidationFacts;

/// Uses ordinary type relations and preserves the existing diagnostic context and callbacks.
#[derive(Clone, Copy, Debug)]
pub(super) struct OrdinaryAssignmentValidationEffects;

ty_mapping_probe_macros::shared_semantic_family! {
    /// Supplies the ordered semantic checks and reports for assignment validation.
    #[synchronous(SynchronousAssignmentValidationEffects)]
    pub(in crate::types::infer) trait AssignmentValidationEffects<'db, 'ast> {
        type Error;

        #[operation(checkpoint)]
        async fn checkpoint(&self) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn assignable(&self, builder: &TypeInferenceBuilder<'db, 'ast>, value: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn invalid_assignment(&self, builder: &TypeInferenceBuilder<'db, 'ast>, request: AssignmentValidationRequest<'_, 'db>) -> Result<(), Self::Error>;
        #[operation(child)]
        async fn unsound_lint_enabled(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn fully_static(&self, builder: &TypeInferenceBuilder<'db, 'ast>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn dataclass_body(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn pure_redundancy(&self, builder: &TypeInferenceBuilder<'db, 'ast>, value: Type<'db>, target: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn unsound_assignment(&self, builder: &TypeInferenceBuilder<'db, 'ast>, request: AssignmentValidationRequest<'_, 'db>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl AssignmentValidationFacts {
        const fn dynamic_target(&self, target: Type<'_>) -> bool {
            matches!(target, Type::Dynamic(_))
        }
    }

    /// Validates the assigned value, retaining a true result when only unsound-assignment reports.
    #[synchronous(validate_assignment_sync)]
    #[capabilities(effects = AssignmentValidationEffects, facts = AssignmentValidationFacts)]
    #[passive_values()]
    pub(in crate::types::infer) async fn validate_assignment_with<'db, 'ast, E: AssignmentValidationEffects<'db, 'ast>>(
        builder: &TypeInferenceBuilder<'db, 'ast>,
        request: AssignmentValidationRequest<'_, 'db>,
        facts: AssignmentValidationFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        effects.checkpoint().await?;
        // A gradual target accepts every value and cannot produce an unsound-assignment
        // diagnostic, which requires a fully static target.
        if facts.dynamic_target(request.target) {
            return Ok(true);
        }
        if !effects.assignable(builder, request.value, request.target).await? {
            effects.invalid_assignment(builder, request).await?;
            return Ok(false);
        }

        // N.B. the implementation here is the ~same as for `UNSOUND_YIELD` and `UNSOUND_RETURN_STATEMENT`;
        // update those too if updating this!
        if effects.unsound_lint_enabled(builder).await?
            && !effects.in_stub(builder).await?
            && effects.fully_static(builder, request.target).await?
            && !effects.dataclass_body(builder).await?
            && !effects.pure_redundancy(builder, request.value, request.target).await?
        {
            effects.unsound_assignment(builder, request).await?;
        }
        Ok(true)
    }
}

impl<'db, 'ast> SynchronousAssignmentValidationEffects<'db, 'ast>
    for OrdinaryAssignmentValidationEffects
{
    type Error = Infallible;

    fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    fn assignable(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        value: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(value.is_assignable_to(builder.db(), builder.program_environment(), target))
    }

    fn invalid_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        request: AssignmentValidationRequest<'_, 'db>,
    ) -> Result<(), Infallible> {
        report_invalid_assignment(
            &builder.context,
            request.node,
            request.binding,
            request.declaration,
            request.target,
            request.value,
        );
        Ok(())
    }

    fn unsound_lint_enabled(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Infallible> {
        Ok(builder.context.is_lint_enabled(&UNSOUND_ASSIGNMENT))
    }

    fn in_stub(&self, builder: &TypeInferenceBuilder<'db, 'ast>) -> Result<bool, Infallible> {
        Ok(builder.file().is_stub(builder.db()))
    }

    fn fully_static(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(target.is_fully_static(builder.db(), builder.program_environment()))
    }

    fn dataclass_body(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> Result<bool, Infallible> {
        Ok(builder.is_in_dataclass_like_class_body())
    }

    fn pure_redundancy(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        value: Type<'db>,
        target: Type<'db>,
    ) -> Result<bool, Infallible> {
        Ok(value.is_pure_redundant_with(builder.db(), builder.program_environment(), target))
    }

    fn unsound_assignment(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        request: AssignmentValidationRequest<'_, 'db>,
    ) -> Result<(), Infallible> {
        report_unsound_assignment(
            &builder.context,
            request.node,
            request.binding,
            request.declaration,
            request.target,
            request.value,
            |expression| builder.expression_type(expression),
        );
        Ok(())
    }
}
