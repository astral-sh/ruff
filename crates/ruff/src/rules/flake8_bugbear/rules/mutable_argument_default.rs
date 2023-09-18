use ast::call_path::{from_qualified_name, CallPath};
use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::{self as ast, Expr, Parameter, ParameterWithDefault};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_index::Indexer;
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_python_trivia::{indentation_at_offset, textwrap};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

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
/// Arguments with immutable type annotations will be ignored by this rule.
/// Types outside of the standard library can be marked as immutable with the
/// [`flake8-bugbear.extend-immutable-calls`] configuration option.
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
/// ## Options
/// - `flake8-bugbear.extend-immutable-calls`
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

        let extend_immutable_calls: Vec<CallPath> = checker
            .settings
            .flake8_bugbear
            .extend_immutable_calls
            .iter()
            .map(|target| from_qualified_name(target))
            .collect();

        if is_mutable_expr(default, checker.semantic())
            && !parameter.annotation.as_ref().is_some_and(|expr| {
                is_immutable_annotation(expr, checker.semantic(), extend_immutable_calls.as_slice())
            })
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
    if indexer.in_multi_statement_line(statement, locator) {
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

    let initialization_edit = if is_docstring_stmt(statement) {
        // If the first statement in the function is a docstring, insert _after_ it.
        if let Some(statement) = body.next() {
            // If there's a second statement, insert _before_ it, but ensure this isn't a
            // multi-statement line.
            if indexer.in_multi_statement_line(statement, locator) {
                return None;
            }
            Edit::insertion(content, locator.line_start(statement.start()))
        } else if locator.full_line_end(statement.end()) == locator.text_len() {
            // If the statement is at the end of the file, without a trailing newline, insert
            // _after_ it with an extra newline.
            Edit::insertion(
                format!("{}{}", stylist.line_ending().as_str(), content),
                locator.full_line_end(statement.end()),
            )
        } else {
            // If the docstring is the only statement, insert _after_ it.
            Edit::insertion(content, locator.full_line_end(statement.end()))
        }
    } else {
        // Otherwise, insert before the first statement.
        let at = locator.line_start(statement.start());
        Edit::insertion(content, at)
    };

    Some(Fix::manual_edits(default_edit, [initialization_edit]))
}
