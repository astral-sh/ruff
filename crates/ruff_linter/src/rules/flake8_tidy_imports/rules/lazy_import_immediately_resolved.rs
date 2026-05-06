use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{ExprName, PythonVersion, Stmt, StmtImport, StmtImportFrom};
use ruff_python_semantic::{Binding, BindingKind, ScopeKind};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

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
///
/// ## Fix safety
/// This rule's fix is marked as unsafe because converting a lazy import to an
/// eager import changes when the imported module is executed, which can change
/// runtime behavior if the module has import-time side effects.
///
/// The fix is only available when the lazy import statement imports a single
/// member, since removing `lazy` from a multi-member import would make every
/// imported member eager, including names that may not be resolved immediately.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.13")]
pub(crate) struct LazyImportImmediatelyResolved {
    name: String,
    fixable: bool,
}

impl Violation for LazyImportImmediatelyResolved {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let LazyImportImmediatelyResolved { name, fixable: _ } = self;
        format!("Lazy import `{name}` is resolved immediately")
    }

    fn fix_title(&self) -> Option<String> {
        self.fixable
            .then(|| "Convert to an eager import".to_string())
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
    let Some(import) = lazy_import_statement(binding, checker) else {
        return;
    };

    let fixable = is_single_member_import(import);

    let mut diagnostic = checker.report_diagnostic(
        LazyImportImmediatelyResolved {
            name: name.id.to_string(),
            fixable,
        },
        name.range(),
    );
    if fixable {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(TextRange::at(
            import.start(),
            TextSize::from(5),
        ))));
    }
}

/// Return the import statement if the binding was created by a `lazy import`.
fn lazy_import_statement<'a>(binding: &Binding, checker: &Checker<'a>) -> Option<&'a Stmt> {
    if !matches!(
        binding.kind,
        BindingKind::Import(_) | BindingKind::SubmoduleImport(_) | BindingKind::FromImport(_)
    ) {
        return None;
    }

    let stmt = binding.statement(checker.semantic())?;
    matches!(
        stmt,
        Stmt::Import(StmtImport { is_lazy: true, .. })
            | Stmt::ImportFrom(StmtImportFrom { is_lazy: true, .. })
    )
    .then_some(stmt)
}

/// Return `true` if removing `lazy` only makes the resolved import eager.
fn is_single_member_import(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Import(StmtImport { names, .. }) | Stmt::ImportFrom(StmtImportFrom { names, .. }) => {
            names.len() == 1
        }
        _ => false,
    }
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
