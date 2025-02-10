use anyhow::Context;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_python_semantic::{Scope, ScopeKind};
use ruff_python_trivia::{indentation_at_offset, textwrap};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::{checkers::ast::Checker, importer::ImportRequest};

use super::helpers::{dataclass_kind, DataclassKind};

/// ## What it does
/// Checks for `__post_init__` dataclass methods with parameter defaults.
///
/// ## Why is this bad?
/// Adding a default value to a parameter in a `__post_init__` method has no
/// impact on whether the parameter will have a default value in the dataclass's
/// generated `__init__` method. To create an init-only dataclass parameter with
/// a default value, you should use an `InitVar` field in the dataclass's class
/// body and give that `InitVar` field a default value.
///
/// As the [documentation] states:
///
/// > Init-only fields are added as parameters to the generated `__init__()`
/// > method, and are passed to the optional `__post_init__()` method. They are
/// > not otherwise used by dataclasses.
///
/// ## Example
/// ```python
/// from dataclasses import InitVar, dataclass
///
///
/// @dataclass
/// class Foo:
///     bar: InitVar[int] = 0
///
///     def __post_init__(self, bar: int = 1, baz: int = 2) -> None:
///         print(bar, baz)
///
///
/// foo = Foo()  # Prints '0 2'.
/// ```
///
/// Use instead:
/// ```python
/// from dataclasses import InitVar, dataclass
///
///
/// @dataclass
/// class Foo:
///     bar: InitVar[int] = 1
///     baz: InitVar[int] = 2
///
///     def __post_init__(self, bar: int, baz: int) -> None:
///         print(bar, baz)
///
///
/// foo = Foo()  # Prints '1 2'.
/// ```
///
/// ## References
/// - [Python documentation: Post-init processing](https://docs.python.org/3/library/dataclasses.html#post-init-processing)
/// - [Python documentation: Init-only variables](https://docs.python.org/3/library/dataclasses.html#init-only-variables)
///
/// [documentation]: https://docs.python.org/3/library/dataclasses.html#init-only-variables
#[derive(ViolationMetadata)]
pub(crate) struct PostInitDefault;

impl Violation for PostInitDefault {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`__post_init__` method with argument defaults".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `dataclasses.InitVar` instead".to_string())
    }
}

/// RUF033
pub(crate) fn post_init_default(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    if &function_def.name != "__post_init__" {
        return;
    }

    let current_scope = checker.semantic().current_scope();
    match current_scope.kind {
        ScopeKind::Class(class_def) => {
            if !matches!(
                dataclass_kind(class_def, checker.semantic()),
                Some((DataclassKind::Stdlib, _))
            ) {
                return;
            }
        }
        _ => return,
    }

    let mut stopped_fixes = false;
    let mut diagnostics = vec![];

    for parameter in function_def.parameters.iter_non_variadic_params() {
        let Some(default) = parameter.default() else {
            continue;
        };
        let mut diagnostic = Diagnostic::new(PostInitDefault, default.range());

        if !stopped_fixes {
            diagnostic.try_set_fix(|| {
                use_initvar(
                    current_scope,
                    function_def,
                    &parameter.parameter,
                    default,
                    checker,
                )
            });
            // Need to stop fixes as soon as there is a parameter we cannot fix.
            // Otherwise, we risk a syntax error (a parameter without a default
            // following parameter with a default).
            stopped_fixes |= diagnostic.fix.is_none();
        }

        diagnostics.push(diagnostic);
    }

    checker.report_diagnostics(diagnostics);
}

/// Generate a [`Fix`] to transform a `__post_init__` default argument into a
/// `dataclasses.InitVar` pseudo-field.
fn use_initvar(
    current_scope: &Scope,
    post_init_def: &ast::StmtFunctionDef,
    parameter: &ast::Parameter,
    default: &ast::Expr,
    checker: &Checker,
) -> anyhow::Result<Fix> {
    if current_scope.has(&parameter.name) {
        return Err(anyhow::anyhow!(
            "Cannot add a `{}: InitVar` field to the class body, as a field by that name already exists",
            parameter.name
        ));
    }

    // Ensure that `dataclasses.InitVar` is accessible. For example,
    // + `from dataclasses import InitVar`
    let (import_edit, initvar_binding) = checker.importer().get_or_import_symbol(
        &ImportRequest::import("dataclasses", "InitVar"),
        default.start(),
        checker.semantic(),
    )?;

    // Delete the default value. For example,
    // - def __post_init__(self, foo: int = 0) -> None: ...
    // + def __post_init__(self, foo: int) -> None: ...
    let default_edit = Edit::deletion(parameter.end(), default.end());

    // Add `dataclasses.InitVar` field to class body.
    let locator = checker.locator();

    let content = {
        let parameter_name = locator.slice(&parameter.name);
        let default = locator.slice(default);
        let line_ending = checker.stylist().line_ending().as_str();

        if let Some(annotation) = &parameter
            .annotation()
            .map(|annotation| locator.slice(annotation))
        {
            format!("{parameter_name}: {initvar_binding}[{annotation}] = {default}{line_ending}")
        } else {
            format!("{parameter_name}: {initvar_binding} = {default}{line_ending}")
        }
    };

    let indentation = indentation_at_offset(post_init_def.start(), checker.source())
        .context("Failed to calculate leading indentation of `__post_init__` method")?;
    let content = textwrap::indent(&content, indentation);

    let initvar_edit = Edit::insertion(
        content.into_owned(),
        locator.line_start(post_init_def.start()),
    );
    Ok(Fix::unsafe_edits(import_edit, [default_edit, initvar_edit]))
}
