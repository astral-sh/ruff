use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::whitespace::trailing_comment_start_offset;
use ruff_python_index::Indexer;
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::has_leading_content;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::edits::{delete_stmt, is_lone_child};
use crate::{Edit, Fix, FixAvailability, Locator, Violation};

/// ## What it does
/// Checks for `print` statements.
///
/// ## Why is this bad?
/// `print` statements used for debugging should be omitted from production
/// code. They can lead the accidental inclusion of sensitive information in
/// logs, and are not configurable by clients, unlike `logging` statements.
///
/// `print` statements used to produce output as a part of a command-line
/// interface program are not typically a problem.
///
/// ## Example
/// ```python
/// def sum_less_than_four(a, b):
///     print(f"Calling sum_less_than_four")
///     return a + b < 4
/// ```
///
/// The automatic fix will remove the print statement entirely:
///
/// ```python
/// def sum_less_than_four(a, b):
///     return a + b < 4
/// ```
///
/// To keep the line for logging purposes, instead use something like:
///
/// ```python
/// import logging
///
/// logger = logging.getLogger(__name__)
///
///
/// def sum_less_than_four(a, b):
///     logger.debug("Calling sum_less_than_four")
///     return a + b < 4
///
///
/// if __name__ == "__main__":
///     logging.basicConfig(level=logging.INFO)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it will remove `print` statements
/// that are used beyond debugging purposes.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.57", category = Category::Restriction)]
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
/// Like `print` statements, `pprint` statements used for debugging should
/// be omitted from production code. They can lead the accidental inclusion
/// of sensitive information in logs, and are not configurable by clients,
/// unlike `logging` statements.
///
/// `pprint` statements used to produce output as a part of a command-line
/// interface program are not typically a problem.
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
/// This rule's fix is marked as unsafe, as it will remove `pprint` statements
/// that are used beyond debugging purposes.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.57", category = Category::Restriction)]
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

    let diagnostic = match qualified_name.segments() {
        ["" | "builtins", "print"] => {
            // If the print call has a `file=` argument (that isn't `None`, `"sys.stdout"`,
            // or `"sys.stderr"`), don't trigger T201.
            if has_non_default_output_target(semantic, call, "file") {
                return;
            }
            checker.report_diagnostic_if_enabled(Print, call.func.range())
        }
        ["pprint", "pprint"] => {
            // If the pprint call has a `stream=` argument (that isn't `None`,
            // `"sys.stdout"`, or `"sys.stderr"`), don't trigger T203.
            if has_non_default_output_target(semantic, call, "stream") {
                return;
            }
            checker.report_diagnostic_if_enabled(PPrint, call.func.range())
        }
        _ => return,
    };

    let Some(mut diagnostic) = diagnostic else {
        return;
    };

    // Remove the `print`, if it's a standalone statement.
    if semantic.current_expression_parent().is_none() {
        let statement = semantic.current_statement();
        let parent = semantic.current_statement_parent();
        let edit = delete_print_stmt(statement, parent, checker.locator(), checker.indexer());
        diagnostic.set_fix(
            Fix::unsafe_edit(edit)
                .isolate(Checker::isolation(semantic.current_statement_parent_id())),
        );
    }
}

/// Delete a `print`/`pprint` statement, retaining any comment that trails it
/// on the same line.
///
/// The default statement deletion removes the statement's full lines, which
/// would take such a comment with it. When the statement ends its line with
/// only a comment following it, and the default deletion would have taken the
/// full-lines path (i.e. the statement is not a lone child, does not share its
/// line with other content, and is not part of a continuation), delete only up
/// to the start of the comment instead, so that the comment and its
/// indentation survive. All other cases defer to [`delete_stmt`] unchanged, so
/// lone children still get a `pass` replacement and multi-statement lines keep
/// their existing handling.
fn delete_print_stmt(
    stmt: &ast::Stmt,
    parent: Option<&ast::Stmt>,
    locator: &Locator,
    indexer: &Indexer,
) -> Edit {
    let full_lines_path = !parent.is_some_and(|parent| is_lone_child(stmt, parent))
        && !has_leading_content(stmt.start(), locator.contents())
        && indexer
            .preceded_by_continuations(stmt.start(), locator.contents())
            .is_none();

    if full_lines_path {
        if let Some(index) = trailing_comment_start_offset(stmt, locator.contents()) {
            return Edit::range_deletion(TextRange::new(stmt.start(), stmt.end() + index));
        }
    }

    delete_stmt(stmt, parent, locator, indexer)
}

fn has_non_default_output_target(
    semantic: &SemanticModel,
    call: &ast::ExprCall,
    keyword: &str,
) -> bool {
    call.arguments.find_keyword(keyword).is_some_and(|keyword| {
        !keyword.value.is_none_literal_expr()
            && semantic
                .resolve_qualified_name(&keyword.value)
                .is_none_or(|qualified_name| {
                    !matches!(qualified_name.segments(), ["sys", "stdout" | "stderr"])
                })
    })
}
