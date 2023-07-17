use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::docstrings::leading_space;
use ruff_python_ast::{ParameterWithDefault, Parameters, Ranged, Stmt};
use ruff_python_parser::lexer::lex_starts_at;
use ruff_python_parser::{Mode, Tok};
use ruff_python_semantic::analyze::typing::{is_immutable_annotation, is_mutable_expr};
use ruff_python_trivia::indentation_at_offset;
use ruff_source_file::Locator;
use ruff_text_size::TextRange;

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
        format!("Replace mutable data structure with `None` in argument default and replace it with data structure inside the function if still `None`")
    }
    fn autofix_title(&self) -> Option<String> {
        Some(format!(
            "Do not use mutable data structures for argument defaults"
        ))
    }
}

/// B006
pub(crate) fn mutable_argument_default(
    checker: &mut Checker,
    parameters: &Parameters,
    body: &[Stmt],
    func_range: TextRange,
) {
    // Scan in reverse order to right-align zip().
    for ParameterWithDefault {
        parameter,
        default,
        range: _,
    } in parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(&parameters.kwonlyargs)
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
            if checker.patch(diagnostic.kind.rule())
                && !is_single_line(checker.locator(), func_range, body)
            {
                // Set the default arg value to None
                let arg_edit = Edit::range_replacement("None".to_string(), default.range());

                // Add conditional check to set the default arg to its original value if still None
                let mut check_lines = String::new();
                let indentation =
                    indentation_at_offset(body[0].start(), checker.locator()).unwrap_or_default();
                let indentation = leading_space(indentation);
                // body[0].start() starts at correct indentation so we do need to add indentation
                // before pushing the if statement
                check_lines.push_str(format!("if {} is None:\n", parameter.name.as_str()).as_str());
                check_lines.push_str(indentation);
                check_lines.push_str(checker.stylist().indentation());
                check_lines.push_str(
                    format!(
                        "{} = {}",
                        parameter.name.as_str(),
                        checker.generator().expr(default),
                    )
                    .as_str(),
                );
                check_lines.push_str(&checker.stylist().line_ending());
                check_lines.push_str(indentation);
                let check_edit = Edit::insertion(check_lines, body[0].start());

                diagnostic.set_fix(Fix::manual_edits(arg_edit, [check_edit]));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}

fn is_single_line(locator: &Locator, func_range: TextRange, body: &[Stmt]) -> bool {
    let arg_string = locator.slice(func_range);
    for (tok, range) in lex_starts_at(arg_string, Mode::Module, func_range.start()).flatten() {
        match tok {
            Tok::Colon => {
                return !locator.contains_line_break(TextRange::new(range.end(), body[0].start()));
            }
            _ => continue,
        }
    }
    false
}
