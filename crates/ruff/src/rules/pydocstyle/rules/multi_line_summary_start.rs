use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::constants;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pydocstyle::helpers::leading_quote;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct MultiLineSummaryFirstLine;
);
impl AlwaysAutofixableViolation for MultiLineSummaryFirstLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multi-line docstring summary should start at the first line")
    }

    fn autofix_title(&self) -> String {
        "Remove whitespace after opening quotes".to_string()
    }
}

define_violation!(
    pub struct MultiLineSummarySecondLine;
);
impl AlwaysAutofixableViolation for MultiLineSummarySecondLine {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multi-line docstring summary should start at the second line")
    }

    fn autofix_title(&self) -> String {
        "Insert line break and indentation after opening quotes".to_string()
    }
}

/// D212, D213
pub fn multi_line_summary_start(checker: &mut Checker, docstring: &Docstring) {
    let contents = docstring.contents;
    let body = docstring.body;

    if LinesWithTrailingNewline::from(body).nth(1).is_none() {
        return;
    };
    let mut content_lines = contents.lines();
    let Some(first_line) = content_lines
        .next()
         else
    {
        return;
    };
    if constants::TRIPLE_QUOTE_PREFIXES.contains(&first_line) {
        if checker
            .settings
            .rules
            .enabled(&Rule::MultiLineSummaryFirstLine)
        {
            let mut diagnostic = Diagnostic::new(
                MultiLineSummaryFirstLine,
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let location = docstring.expr.location;
                let mut end_row = location.row() + 1;
                // Delete until first non-whitespace char.
                for line in content_lines {
                    if let Some(end_column) = line.find(|c: char| !c.is_whitespace()) {
                        let start =
                            Location::new(location.row(), location.column() + first_line.len());
                        let end = Location::new(end_row, end_column);
                        diagnostic.amend(Fix::deletion(start, end));
                        break;
                    }
                    end_row += 1;
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    } else {
        if checker
            .settings
            .rules
            .enabled(&Rule::MultiLineSummarySecondLine)
        {
            let mut diagnostic = Diagnostic::new(
                MultiLineSummarySecondLine,
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let mut indentation = String::from(docstring.indentation);
                let mut fixable = true;
                if !indentation.chars().all(char::is_whitespace) {
                    fixable = false;

                    // If the docstring isn't on its own line, look at the parent indentation, and
                    // add the default indentation to get the "right" level.
                    if let DefinitionKind::Class(parent)
                    | DefinitionKind::NestedClass(parent)
                    | DefinitionKind::Function(parent)
                    | DefinitionKind::NestedFunction(parent)
                    | DefinitionKind::Method(parent) = &docstring.kind
                    {
                        let parent_indentation =
                            checker.locator.slice_source_code_range(&Range::new(
                                Location::new(parent.location.row(), 0),
                                Location::new(parent.location.row(), parent.location.column()),
                            ));
                        if parent_indentation.chars().all(char::is_whitespace) {
                            indentation.clear();
                            indentation.push_str(parent_indentation);
                            indentation.push_str(checker.stylist.indentation());
                            fixable = true;
                        }
                    };
                }

                if fixable {
                    let location = docstring.expr.location;
                    let prefix = leading_quote(contents).unwrap();
                    // Use replacement instead of insert to trim possible whitespace between leading
                    // quote and text.
                    let repl = format!(
                        "{}{}{}",
                        checker.stylist.line_ending().as_str(),
                        indentation,
                        first_line.strip_prefix(prefix).unwrap().trim_start()
                    );
                    diagnostic.amend(Fix::replacement(
                        repl,
                        Location::new(location.row(), location.column() + prefix.len()),
                        Location::new(location.row(), location.column() + first_line.len()),
                    ));
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
