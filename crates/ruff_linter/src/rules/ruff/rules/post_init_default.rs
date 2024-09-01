use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    self as ast, helpers::is_docstring_stmt, Expr, Parameter, ParameterWithDefault, Stmt,
};
use ruff_python_trivia::{indentation_at_offset, textwrap};
use ruff_text_size::{Ranged, TextRange};

use crate::{checkers::ast::Checker, importer::ImportRequest};

use super::helpers::is_dataclass;

/// ## What it does
/// Checks for `__post_init__` dataclass methods with argument defaults.
///
/// ## Why is this bad?
/// Variables that are only used during initialization should be instantiated
/// as an init-only pseudo-field using `dataclasses.InitVar`. According to the
/// [documentation]:
///
/// > Init-only fields are added as parameters to the generated `__init__()`
/// > method, and are passed to the optional `__post_init__()` method. They are
/// > not otherwise used by dataclasses.
///
/// Default values for `__post_init__` arguments that exist as init-only fields
/// as well will be overridden by the `dataclasses.InitVar` value.
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
#[violation]
pub struct PostInitDefault;

impl Violation for PostInitDefault {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`__post_init__` method with argument defaults")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Use `dataclasses.InitVar` instead"))
    }
}

/// RUF033
pub(crate) fn post_init_default(checker: &mut Checker, class_def: &ast::StmtClassDef) {
    if !is_dataclass(class_def, checker.semantic()) {
        return;
    }

    for statement in &class_def.body {
        if let Stmt::FunctionDef(function_def) = statement {
            if &function_def.name != "__post_init__" {
                continue;
            }

            let mut stopped_fixes = false;

            for ParameterWithDefault {
                parameter,
                default,
                range,
            } in function_def.parameters.iter_non_variadic_params()
            {
                let Some(default) = default else {
                    continue;
                };
                let mut diagnostic = Diagnostic::new(PostInitDefault, default.range());

                // If the name is already bound on the class, no fix is available.
                if !already_bound(parameter, class_def) && !stopped_fixes {
                    if let Some(fix) = use_initvar(class_def, parameter, default, *range, checker) {
                        diagnostic.set_fix(fix);
                    }
                } else {
                    // Need to stop fixes as soon as there is a parameter we
                    // cannot fix. Otherwise, we risk a syntax error (a
                    // parameter without a default following parameter with a
                    // default).
                    stopped_fixes = true;
                }

                checker.diagnostics.push(diagnostic);
            }
        }
    }
}

/// Generate a [`Fix`] to transform a `__post_init__` default argument into a
/// `dataclasses.InitVar` pseudo-field.
fn use_initvar(
    class_def: &ast::StmtClassDef,
    parameter: &Parameter,
    default: &Expr,
    range: TextRange,
    checker: &Checker,
) -> Option<Fix> {
    // Ensure that `dataclasses.InitVar` is accessible. For example,
    // + `from dataclasses import InitVar`
    let (import_edit, binding) = checker
        .importer()
        .get_or_import_symbol(
            &ImportRequest::import("dataclasses", "InitVar"),
            default.start(),
            checker.semantic(),
        )
        .ok()?;

    // Delete the default value. For example,
    // - def __post_init__(self, foo: int = 0) -> None: ...
    // + def __post_init__(self, foo: int) -> None: ...
    let (default_start, default_end) = match &parameter.annotation {
        Some(annotation) => (annotation.range().end(), range.end()),
        None => (parameter.range().end(), range.end()),
    };
    let default_edit = Edit::deletion(default_start, default_end);

    // Add `dataclasses.InitVar` field to class body.
    let mut body = class_def.body.iter().peekable();
    let statement = body.peek()?;
    let mut content = String::new();
    match &parameter.annotation {
        Some(annotation) => {
            content.push_str(&format!(
                "{}: {}[{}] = {}",
                checker.locator().slice(&parameter.name),
                binding,
                checker.locator().slice(annotation.range()),
                checker.locator().slice(default)
            ));
        }
        None => {
            content.push_str(&format!(
                "{}: {} = {}",
                checker.locator().slice(&parameter.name),
                binding,
                checker.locator().slice(default)
            ));
        }
    }
    content.push_str(checker.stylist().line_ending().as_str());
    let indentation = indentation_at_offset(statement.start(), checker.locator())?;
    let content = textwrap::indent(&content, indentation).to_string();

    // Find the position after any docstring to insert the field.
    let mut pos = checker.locator().line_start(statement.start());
    while let Some(statement) = body.next() {
        if is_docstring_stmt(statement) {
            if let Some(next) = body.peek() {
                if checker
                    .indexer()
                    .in_multi_statement_line(statement, checker.locator())
                {
                    continue;
                }
                pos = checker.locator().line_start(next.start());
            } else {
                pos = checker.locator().full_line_end(statement.end());
                break;
            }
        } else {
            break;
        };
    }
    let initvar_edit = Edit::insertion(content, pos);

    Some(Fix::unsafe_edits(import_edit, [default_edit, initvar_edit]))
}

/// Check if a name is already bound as a class variable.
fn already_bound(parameter: &Parameter, class_def: &ast::StmtClassDef) -> bool {
    for statement in &class_def.body {
        let target = match statement {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                if let [Expr::Name(id)] = targets.as_slice() {
                    id
                } else {
                    continue;
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Name(id) = target.as_ref() {
                    id
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        if *target.id == *parameter.name {
            return true;
        }
    }
    false
}
