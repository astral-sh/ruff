use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprName, PythonVersion, Stmt, StmtImport, StmtImportFrom};
use ruff_python_semantic::{Binding, BindingKind, ScopeKind};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for lazy imports that are resolved immediately at module load time.
///
/// ## Why is this bad?
/// Python 3.15 adds support for `lazy import` and `lazy from ... import ...`,
/// which defer the actual import work until the imported name is first used.
///
/// When a lazily imported name is used immediately at module load time, the
/// import is resolved eagerly, defeating the purpose of marking the import as
/// lazy. This is commonly caused by using a lazily imported module in a module
/// global or in a top-level class definition.
///
/// ## Example
/// ```python
/// lazy import foo
///
///
/// class Bar(foo.Foo): ...
/// ```
///
/// Use instead:
/// ```python
/// import foo
///
///
/// class Bar(foo.Foo): ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.12")]
pub(crate) struct LazyImportImmediatelyResolved {
    name: String,
}

impl Violation for LazyImportImmediatelyResolved {
    #[derive_message_formats]
    fn message(&self) -> String {
        let LazyImportImmediatelyResolved { name } = self;
        format!("Lazy import `{name}` is resolved immediately")
    }
}

/// TID255
/// Check whether a name load forces a lazy import during module import execution.
pub(crate) fn lazy_import_immediately_resolved(checker: &Checker, name: &ExprName) {
    if checker.target_version() < PythonVersion::PY315 {
        return;
    }

    if !is_immediate_resolution_context(checker) {
        return;
    }

    let Some(binding_id) = checker.semantic().resolve_name(name) else {
        return;
    };

    let binding = checker.semantic().binding(binding_id);
    if !is_lazy_import(binding, checker) {
        return;
    }

    checker.report_diagnostic(
        LazyImportImmediatelyResolved {
            name: name.id.to_string(),
        },
        name.range(),
    );
}

/// Return `true` if the binding was created by a `lazy import` statement.
fn is_lazy_import(binding: &Binding, checker: &Checker) -> bool {
    if !matches!(
        binding.kind,
        BindingKind::Import(_) | BindingKind::SubmoduleImport(_) | BindingKind::FromImport(_)
    ) {
        return false;
    }

    matches!(
        binding.statement(checker.semantic()),
        Some(
            Stmt::Import(StmtImport { is_lazy: true, .. })
                | Stmt::ImportFrom(StmtImportFrom { is_lazy: true, .. })
        )
    )
}

/// Return `true` if the current expression is evaluated during module import execution.
fn is_immediate_resolution_context(checker: &Checker) -> bool {
    let semantic = checker.semantic();

    if checker.source_type.is_stub()
        || !semantic.execution_context().is_runtime()
        || semantic.in_deferred_type_definition()
        || semantic.in_deferred_type_alias_value()
    {
        return false;
    }

    if semantic.in_exception_handler() || in_conditional_block(checker) {
        return false;
    }

    match semantic.current_scope().kind {
        ScopeKind::Module => true,
        ScopeKind::Class(_) | ScopeKind::Type => semantic
            .first_non_type_parent_scope(semantic.current_scope())
            .is_some_and(|scope| scope.kind.is_module()),
        ScopeKind::Function(_)
        | ScopeKind::Lambda(_)
        | ScopeKind::Generator { .. }
        | ScopeKind::DunderClassCell => false,
    }
}

/// Return `true` if the current expression is inside a conditional block.
fn in_conditional_block(checker: &Checker) -> bool {
    checker
        .semantic()
        .current_statements()
        .skip(1)
        .any(|parent| matches!(parent, Stmt::If(_) | Stmt::While(_) | Stmt::Match(_)))
}
