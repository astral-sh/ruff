use ruff_python_ast::StmtFunctionDef;
use ruff_python_semantic::{ScopeKind, SemanticModel};

use crate::rules::flake8_type_checking;
use crate::settings::LinterSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AnnotationContext {
    /// Python will evaluate the annotation at runtime, but it's not _required_ and, as such, could
    /// be quoted to convert it into a typing-only annotation.
    ///
    /// For example:
    /// ```python
    /// from pandas import DataFrame
    ///
    /// def foo() -> DataFrame:
    ///    ...
    /// ```
    ///
    /// Above, Python will evaluate `DataFrame` at runtime in order to add it to `__annotations__`.
    RuntimeEvaluated,
    /// Python will evaluate the annotation at runtime, and it's required to be available at
    /// runtime, as a library (like Pydantic) needs access to it.
    RuntimeRequired,
    /// The annotation is only evaluated at type-checking time.
    TypingOnly,
}

impl AnnotationContext {
    /// Determine the [`AnnotationContext`] for an annotation based on the current scope of the
    /// semantic model.
    pub(super) fn from_model(semantic: &SemanticModel, settings: &LinterSettings) -> Self {
        // If the annotation is in a class scope (e.g., an annotated assignment for a
        // class field) or a function scope, and that class or function is marked as
        // runtime-required, treat the annotation as runtime-required.
        match semantic.current_scope().kind {
            ScopeKind::Class(class_def)
                if flake8_type_checking::helpers::runtime_required_class(
                    class_def,
                    &settings.flake8_type_checking.runtime_required_base_classes,
                    &settings.flake8_type_checking.runtime_required_decorators,
                    semantic,
                ) =>
            {
                return Self::RuntimeRequired
            }
            ScopeKind::Function(function_def)
                if flake8_type_checking::helpers::runtime_required_function(
                    function_def,
                    &settings.flake8_type_checking.runtime_required_decorators,
                    semantic,
                ) =>
            {
                return Self::RuntimeRequired
            }
            _ => {}
        }

        // If `__future__` annotations are enabled or it's a stub file,
        // then annotations are never evaluated at runtime,
        // so we can treat them as typing-only.
        if semantic.future_annotations_or_stub() {
            return Self::TypingOnly;
        }

        // Otherwise, if we're in a class or module scope, then the annotation needs to
        // be available at runtime.
        // See: https://docs.python.org/3/reference/simple_stmts.html#annotated-assignment-statements
        if matches!(
            semantic.current_scope().kind,
            ScopeKind::Class(_) | ScopeKind::Module
        ) {
            return Self::RuntimeEvaluated;
        }

        Self::TypingOnly
    }

    /// Determine the [`AnnotationContext`] to use for annotations in a function signature.
    pub(super) fn from_function(
        function_def: &StmtFunctionDef,
        semantic: &SemanticModel,
        settings: &LinterSettings,
    ) -> Self {
        if flake8_type_checking::helpers::runtime_required_function(
            function_def,
            &settings.flake8_type_checking.runtime_required_decorators,
            semantic,
        ) {
            Self::RuntimeRequired
        } else if semantic.future_annotations_or_stub() {
            Self::TypingOnly
        } else {
            Self::RuntimeEvaluated
        }
    }
}
