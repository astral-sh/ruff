use ruff_diagnostics::{Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast as ast;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::delete_stmt;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `print` statements.
///
/// ## Why is this bad?
/// `print` statements are useful in some situations (e.g., debugging), but
/// should typically be omitted from production code. `print` statements can
/// lead to the accidental inclusion of sensitive information in logs, and are
/// not configurable by clients, unlike `logging` statements.
///
/// ## Example
/// ```python
/// def add_numbers(a, b):
///     print(f"The sum of {a} and {b} is {a + b}")
///     return a + b
/// ```
///
/// Use instead:
/// ```python
/// def add_numbers(a, b):
///     return a + b
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may remove `print` statements
/// that are used beyond debugging purposes.
#[derive(ViolationMetadata)]
pub(crate) struct Print;

impl Violation for Print {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`print` found".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove `print`".to_string())
    }
}

/// ## What it does
/// Checks for `pprint` statements.
///
/// ## Why is this bad?
/// Like `print` statements, `pprint` statements are useful in some situations
/// (e.g., debugging), but should typically be omitted from production code.
/// `pprint` statements can lead to the accidental inclusion of sensitive
/// information in logs, and are not configurable by clients, unlike `logging`
/// statements.
///
/// ## Example
/// ```python
/// import pprint
///
///
/// def merge_dicts(dict_a, dict_b):
///     dict_c = {**dict_a, **dict_b}
///     pprint.pprint(dict_c)
///     return dict_c
/// ```
///
/// Use instead:
/// ```python
/// def merge_dicts(dict_a, dict_b):
///     dict_c = {**dict_a, **dict_b}
///     return dict_c
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may remove `pprint` statements
/// that are used beyond debugging purposes.
#[derive(ViolationMetadata)]
pub(crate) struct PPrint;

impl Violation for PPrint {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`pprint` found".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove `pprint`".to_string())
    }
}

/// T201, T203
pub(crate) fn print_call(checker: &Checker, call: &ast::ExprCall) {
    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(&call.func) else {
        return;
    };

    let mut diagnostic = match qualified_name.segments() {
        ["" | "builtins", "print"] => {
            // If the print call has a `file=` argument (that isn't `None`, `"sys.stdout"`,
            // or `"sys.stderr"`), don't trigger T201.
            if let Some(keyword) = call.arguments.find_keyword("file") {
                if !keyword.value.is_none_literal_expr() {
                    if semantic.resolve_qualified_name(&keyword.value).is_none_or(
                        |qualified_name| {
                            !matches!(qualified_name.segments(), ["sys", "stdout" | "stderr"])
                        },
                    ) {
                        return;
                    }
                }
            }
            Diagnostic::new(Print, call.func.range())
        }
        ["pprint", "pprint"] => Diagnostic::new(PPrint, call.func.range()),
        _ => return,
    };

    if !checker.enabled(diagnostic.kind.rule()) {
        return;
    }

    // Remove the `print`, if it's a standalone statement.
    if semantic.current_expression_parent().is_none() {
        let statement = semantic.current_statement();
        let parent = semantic.current_statement_parent();
        let edit = delete_stmt(statement, parent, checker.locator(), checker.indexer());
        diagnostic.set_fix(
            Fix::unsafe_edit(edit)
                .isolate(Checker::isolation(semantic.current_statement_parent_id())),
        );
    }

    checker.report_diagnostic(diagnostic);
}
