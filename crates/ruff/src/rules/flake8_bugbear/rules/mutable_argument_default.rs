use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::{self as ast, Expr, Parameter, ParameterWithDefault, Ranged};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_index::Indexer;
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_python_trivia::{indentation_at_offset, textwrap};
use ruff_source_file::Locator;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for uses of mutable objects as function argument defaults.
///
/// ## Why is this bad?
/// Function defaults are evaluated once, when the function is defined.
///
/// The same mutable object is then shared across all calls to the function.
/// If the object is modified, those modifications will persist across calls,
/// which can lead to unexpected behavior.
///
/// Instead, prefer to use immutable data structures, or take `None` as a
/// default, and initialize a new mutable object inside the function body
/// for each call.
///
/// ## Example
/// ```python
/// def add_to_list(item, some_list=[]):
///     some_list.append(item)
///     return some_list
///
///
/// l1 = add_to_list(0)  # [0]
/// l2 = add_to_list(1)  # [0, 1]
/// ```
///
/// Use instead:
/// ```python
/// def add_to_list(item, some_list=None):
///     if some_list is None:
///         some_list = []
///     some_list.append(item)
///     return some_list
///
///
/// l1 = add_to_list(0)  # [0]
/// l2 = add_to_list(1)  # [1]
/// ```
///
/// ## References
/// - [Python documentation: Default Argument Values](https://docs.python.org/3/tutorial/controlflow.html#default-argument-values)
#[violation]
pub struct MutableArgumentDefault;

impl Violation for MutableArgumentDefault {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not use mutable data structures for argument defaults")
    }
    fn autofix_title(&self) -> Option<String> {
        Some(format!("Replace with `None`; initialize within function"))
    }
}

/// B006
pub(crate) fn mutable_argument_default(checker: &mut Checker, function_def: &ast::StmtFunctionDef) {
    // Scan in reverse order to right-align zip().
    for ParameterWithDefault {
        parameter,
        default,
        range: _,
    } in function_def
        .parameters
        .posonlyargs
        .iter()
        .chain(&function_def.parameters.args)
        .chain(&function_def.parameters.kwonlyargs)
    {
        let Some(default) = default else {
            continue;
        };

        if is_mutable_expr(default, checker.semantic())
            && !parameter
                .annotation
                .as_ref()
                .is_some_and(|expr| is_immutable_annotation(expr, checker.semantic()))
        {
            let mut diagnostic = Diagnostic::new(MutableArgumentDefault, default.range());

            // If the function body is on the same line as the function def, do not fix
            if checker.patch(diagnostic.kind.rule()) {
                if let Some(fix) = move_initialization(
                    function_def,
                    parameter,
                    default,
                    checker.locator(),
                    checker.stylist(),
                    checker.indexer(),
                    checker.generator(),
                ) {
                    diagnostic.set_fix(fix);
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// Generate a [`Fix`] to move a mutable argument default initialization
/// into the function body.
fn move_initialization(
    function_def: &ast::StmtFunctionDef,
    parameter: &Parameter,
    default: &Expr,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    generator: Generator,
) -> Option<Fix> {
    let mut body = function_def.body.iter();

    let statement = body.next()?;
    if indexer.preceded_by_multi_statement_line(statement, locator) {
        return None;
    }

    // Determine the indentation depth of the function body.
    let indentation = indentation_at_offset(statement.start(), locator)?;

    // Set the default argument value to `None`.
    let default_edit = Edit::range_replacement("None".to_string(), default.range());

    // Add an `if`, to set the argument to its original value if still `None`.
    let mut content = String::new();
    content.push_str(&format!("if {} is None:", parameter.name.as_str()));
    content.push_str(stylist.line_ending().as_str());
    content.push_str(stylist.indentation());
    content.push_str(&format!(
        "{} = {}",
        parameter.name.as_str(),
        generator.expr(default)
    ));
    content.push_str(stylist.line_ending().as_str());

    // Indent the edit to match the body indentation.
    let content = textwrap::indent(&content, indentation).to_string();

    let initialization_pos = if is_docstring_stmt(statement) {
        // If the first statement in the function is a docstring, insert _after_ it.
        if let Some(statement) = body.next() {
            // If there's a second statement, insert _before_ it, but ensure this isn't a
            // multi-statement line.
            if indexer.preceded_by_multi_statement_line(statement, locator) {
                return None;
            }
            locator.line_start(statement.start())
        } else {
            // If the docstring is the only statement, insert _before_ it.
            locator.full_line_end(statement.end())
        }
    } else if statement.is_import_stmt() || statement.is_import_from_stmt() {
        let mut pos = statement.end();
        for stmt in body {
            if stmt.is_import_stmt() || stmt.is_import_from_stmt() {
                pos = stmt.end();
            } else {
                break
            }
        }
        locator.full_line_end(pos)
    } else {
        // Otherwise, insert before the first statement.
        locator.line_start(statement.start())
    };

    let initialization_edit = Edit::insertion(content, initialization_pos);
    Some(Fix::manual_edits(default_edit, [initialization_edit]))
}
