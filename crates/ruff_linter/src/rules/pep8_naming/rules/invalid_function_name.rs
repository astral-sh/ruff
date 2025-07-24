use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Decorator, Stmt, identifier::Identifier};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::{class::any_base_class, visibility};
use ruff_python_stdlib::str;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::pep8_naming::settings::IgnoreNames;

/// ## What it does
/// Checks for functions names that do not follow the `snake_case` naming
/// convention.
///
/// ## Why is this bad?
/// [PEP 8] recommends that function names follow `snake_case`:
///
/// > Function names should be lowercase, with words separated by underscores as necessary to
/// > improve readability. mixedCase is allowed only in contexts where that’s already the
/// > prevailing style (e.g. threading.py), to retain backwards compatibility.
///
/// Names can be excluded from this rule using the [`lint.pep8-naming.ignore-names`]
/// or [`lint.pep8-naming.extend-ignore-names`] configuration options. For example,
/// to ignore all functions starting with `test_` from this rule, set the
/// [`lint.pep8-naming.extend-ignore-names`] option to `["test_*"]`.
///
/// ## Example
/// ```python
/// def myFunction():
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def my_function():
///     pass
/// ```
///
/// ## Options
/// - `lint.pep8-naming.ignore-names`
/// - `lint.pep8-naming.extend-ignore-names`
///
/// [PEP 8]: https://peps.python.org/pep-0008/#function-and-variable-names
#[derive(ViolationMetadata)]
pub(crate) struct InvalidFunctionName {
    name: String,
}

impl Violation for InvalidFunctionName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InvalidFunctionName { name } = self;
        format!("Function name `{name}` should be lowercase")
    }
}

/// N802
pub(crate) fn invalid_function_name(
    checker: &Checker,
    stmt: &Stmt,
    name: &str,
    decorator_list: &[Decorator],
    ignore_names: &IgnoreNames,
    semantic: &SemanticModel,
) {
    // Ignore any function names that are already lowercase.
    if str::is_lowercase(name) {
        return;
    }

    // Ignore any functions that are explicitly `@override` or `@overload`.
    // These are defined elsewhere, so if they're first-party,
    // we'll flag them at the definition site.
    if visibility::is_override(decorator_list, semantic)
        || visibility::is_overload(decorator_list, semantic)
    {
        return;
    }

    // Ignore any explicitly-allowed names.
    if ignore_names.matches(name) {
        return;
    }

    let parent_class = semantic
        .current_statement_parent()
        .and_then(|parent| parent.as_class_def_stmt());

    // Ignore the visit_* methods of the ast.NodeVisitor and ast.NodeTransformer classes.
    if name.starts_with("visit_")
        && parent_class.is_some_and(|class| {
            any_base_class(class, semantic, &mut |superclass| {
                let qualified = semantic.resolve_qualified_name(superclass);
                qualified.is_some_and(|name| {
                    matches!(name.segments(), ["ast", "NodeVisitor" | "NodeTransformer"])
                })
            })
        })
    {
        return;
    }

    // Ignore the do_* methods of the http.server.BaseHTTPRequestHandler class and its subclasses
    if name.starts_with("do_")
        && parent_class.is_some_and(|class| {
            any_base_class(class, semantic, &mut |superclass| {
                let qualified = semantic.resolve_qualified_name(superclass);
                qualified.is_some_and(|name| {
                    matches!(
                        name.segments(),
                        [
                            "http",
                            "server",
                            "BaseHTTPRequestHandler"
                                | "CGIHTTPRequestHandler"
                                | "SimpleHTTPRequestHandler"
                        ]
                    )
                })
            })
        })
    {
        return;
    }

    checker.report_diagnostic(
        InvalidFunctionName {
            name: name.to_string(),
        },
        stmt.identifier(),
    );
}
