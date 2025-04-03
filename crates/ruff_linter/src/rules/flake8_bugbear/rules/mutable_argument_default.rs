use std::fmt::Write;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, Parameter};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_index::Indexer;
use ruff_python_semantic::analyze::function_type::is_stub;
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{indentation_at_offset, textwrap};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::Locator;

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
/// [`lint.flake8-bugbear.extend-immutable-calls`] configuration option.
///
/// ## Known problems
/// Mutable argument defaults can be used intentionally to cache computation
/// results. Replacing the default with `None` or an immutable data structure
/// does not work for such usages. Instead, prefer the `@functools.lru_cache`
/// decorator from the standard library.
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
/// - `lint.flake8-bugbear.extend-immutable-calls`
///
/// ## References
/// - [Python documentation: Default Argument Values](https://docs.python.org/3/tutorial/controlflow.html#default-argument-values)
#[derive(ViolationMetadata)]
pub(crate) struct MutableArgumentDefault;

impl Violation for MutableArgumentDefault {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not use mutable data structures for argument defaults".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `None`; initialize within function".to_string())
    }
}

/// B006
pub(crate) fn mutable_argument_default(checker: &Checker, function_def: &ast::StmtFunctionDef) {
    // Skip stub files
    if checker.source_type.is_stub() {
        return;
    }

    for parameter in function_def.parameters.iter_non_variadic_params() {
        let Some(default) = parameter.default() else {
            continue;
        };

        let extend_immutable_calls: Vec<QualifiedName> = checker
            .settings
            .flake8_bugbear
            .extend_immutable_calls
            .iter()
            .map(|target| QualifiedName::from_dotted_name(target))
            .collect();

        if is_mutable_expr(default, checker.semantic())
            && !parameter.annotation().is_some_and(|expr| {
                is_immutable_annotation(expr, checker.semantic(), extend_immutable_calls.as_slice())
            })
        {
            let mut diagnostic = Diagnostic::new(MutableArgumentDefault, default.range());

            // If the function body is on the same line as the function def, do not fix
            if let Some(fix) = move_initialization(
                function_def,
                &parameter.parameter,
                default,
                checker.semantic(),
                checker.locator(),
                checker.stylist(),
                checker.indexer(),
                checker.generator(),
            ) {
                diagnostic.set_fix(fix);
            }
            checker.report_diagnostic(diagnostic);
        }
    }
}

/// Generate a [`Fix`] to move a mutable argument default initialization
/// into the function body.
#[allow(clippy::too_many_arguments)]
fn move_initialization(
    function_def: &ast::StmtFunctionDef,
    parameter: &Parameter,
    default: &Expr,
    semantic: &SemanticModel,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    generator: Generator,
) -> Option<Fix> {
    let mut body = function_def.body.iter().peekable();

    // Avoid attempting to fix single-line functions.
    let statement = body.peek()?;
    if indexer.preceded_by_multi_statement_line(statement, locator.contents()) {
        return None;
    }

    // Set the default argument value to `None`.
    let default_edit = Edit::range_replacement("None".to_string(), default.range());

    // If the function is a stub, this is the only necessary edit.
    if is_stub(function_def, semantic) {
        return Some(Fix::unsafe_edit(default_edit));
    }

    // Add an `if`, to set the argument to its original value if still `None`.
    let mut content = String::new();
    let _ = write!(&mut content, "if {} is None:", parameter.name());
    content.push_str(stylist.line_ending().as_str());
    content.push_str(stylist.indentation());
    let _ = write!(
        &mut content,
        "{} = {}",
        parameter.name(),
        generator.expr(default)
    );
    content.push_str(stylist.line_ending().as_str());

    // Determine the indentation depth of the function body.
    let indentation = indentation_at_offset(statement.start(), locator.contents())?;

    // Indent the edit to match the body indentation.
    let mut content = textwrap::indent(&content, indentation).to_string();

    // Find the position to insert the initialization after docstring and imports
    let mut pos = locator.line_start(statement.start());
    while let Some(statement) = body.next() {
        // If the statement is a docstring or an import, insert _after_ it.
        if is_docstring_stmt(statement)
            || statement.is_import_stmt()
            || statement.is_import_from_stmt()
        {
            if let Some(next) = body.peek() {
                // If there's a second statement, insert _before_ it, but ensure this isn't a
                // multi-statement line.
                if indexer.in_multi_statement_line(statement, locator.contents()) {
                    continue;
                }
                pos = locator.line_start(next.start());
            } else if locator.full_line_end(statement.end()) == locator.text_len() {
                // If the statement is at the end of the file, without a trailing newline, insert
                // _after_ it with an extra newline.
                content = format!("{}{}", stylist.line_ending().as_str(), content);
                pos = locator.full_line_end(statement.end());
                break;
            } else {
                // If this is the only statement, insert _after_ it.
                pos = locator.full_line_end(statement.end());
                break;
            }
        } else {
            break;
        }
    }

    let initialization_edit = Edit::insertion(content, pos);
    Some(Fix::unsafe_edits(default_edit, [initialization_edit]))
}
