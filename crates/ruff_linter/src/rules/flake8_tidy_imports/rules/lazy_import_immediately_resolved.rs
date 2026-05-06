use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    ExprName, PySourceType, PythonVersion, Stmt, StmtImport, StmtImportFrom, helpers,
    token::{TokenKind, Tokens},
};
use ruff_python_semantic::{Binding, BindingKind, GeneratorKind, ScopeKind, SemanticModel};
use ruff_text_size::{Ranged, TextRange};

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

    let semantic = checker.semantic();

    if !is_immediate_resolution_context(semantic, checker.source_type) {
        return;
    }

    let Some(binding_id) = semantic.resolve_name(name) else {
        return;
    };

    let binding = semantic.binding(binding_id);
    let Some(import) = lazy_import_statement(binding, semantic) else {
        return;
    };

    let fix_range = if is_single_member_import(import) {
        lazy_import_prefix_range(import, checker.source_tokens())
    } else {
        None
    };

    let mut diagnostic = checker.report_diagnostic(
        LazyImportImmediatelyResolved {
            name: name.id.to_string(),
            fixable: fix_range.is_some(),
        },
        name.range(),
    );
    if let Some(fix_range) = fix_range {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::range_deletion(fix_range)));
    }
}

/// Return the import statement if the binding was created by a `lazy import`.
fn lazy_import_statement<'a>(binding: &Binding, semantic: &SemanticModel<'a>) -> Option<&'a Stmt> {
    if !matches!(
        binding.kind,
        BindingKind::Import(_) | BindingKind::SubmoduleImport(_) | BindingKind::FromImport(_)
    ) {
        return None;
    }

    let stmt = binding.statement(semantic)?;
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

/// Return the range from `lazy` through the whitespace before `import` or `from`.
fn lazy_import_prefix_range(stmt: &Stmt, source_tokens: &Tokens) -> Option<TextRange> {
    let mut tokens = source_tokens.after(stmt.start()).iter();
    let lazy = tokens.find(|token| !token.kind().is_trivia())?;
    if lazy.kind() != TokenKind::Lazy {
        return None;
    }

    let import = tokens.find(|token| !token.kind().is_trivia())?;
    matches!(import.kind(), TokenKind::Import | TokenKind::From)
        .then(|| TextRange::new(lazy.start(), import.start()))
}

/// Return `true` if the current expression is evaluated during module import execution.
fn is_immediate_resolution_context(semantic: &SemanticModel, source_type: PySourceType) -> bool {
    if source_type.is_stub()
        || !semantic.execution_context().is_runtime()
        || semantic.in_deferred_type_definition()
        || semantic.in_deferred_type_alias_value()
    {
        return false;
    }

    let mut parent_statements = semantic.current_statements().skip(1);
    if semantic.in_exception_handler() || helpers::on_conditional_branch(&mut parent_statements) {
        return false;
    }

    match semantic.current_scope().kind {
        ScopeKind::Module => true,
        ScopeKind::Class(_) | ScopeKind::Type => semantic
            .first_non_type_parent_scope(semantic.current_scope())
            .is_some_and(|scope| scope.kind.is_module()),
        ScopeKind::Generator {
            kind:
                GeneratorKind::ListComprehension
                | GeneratorKind::DictComprehension
                | GeneratorKind::SetComprehension,
            ..
        } => in_immediate_eager_comprehension_context(semantic),
        ScopeKind::Function(_)
        | ScopeKind::Lambda(_)
        | ScopeKind::Generator {
            kind: GeneratorKind::Generator,
            ..
        }
        | ScopeKind::DunderClassCell => false,
    }
}

fn in_immediate_eager_comprehension_context(semantic: &SemanticModel) -> bool {
    for scope in semantic.current_scopes().skip(1) {
        match scope.kind {
            ScopeKind::Type | ScopeKind::DunderClassCell => {}
            ScopeKind::Generator {
                kind:
                    GeneratorKind::ListComprehension
                    | GeneratorKind::DictComprehension
                    | GeneratorKind::SetComprehension,
                ..
            } => {}
            ScopeKind::Module => return true,
            ScopeKind::Class(_) => {
                return semantic
                    .first_non_type_parent_scope(scope)
                    .is_some_and(|scope| scope.kind.is_module());
            }
            ScopeKind::Function(_)
            | ScopeKind::Lambda(_)
            | ScopeKind::Generator {
                kind: GeneratorKind::Generator,
                ..
            } => return false,
        }
    }

    false
}
