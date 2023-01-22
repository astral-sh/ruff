use crate::ast::types::Range;
use crate::ast::whitespace::LinesWithTrailingNewline;
use crate::checkers::ast::Checker;
use crate::docstrings::constants;
use crate::docstrings::definition::Docstring;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::{Diagnostic, Rule};
use crate::rules::pydocstyle::helpers::leading_quote;
use crate::violations;

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
                violations::MultiLineSummaryFirstLine,
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let loc = docstring.expr.location;
                let mut endl = loc.row() + 1;
                // Delete until first non-whitespace char.
                for line in content_lines {
                    if let Some(endc) = line.find(|c: char| !c.is_whitespace()) {
                        let start = Location::new(loc.row(), loc.column() + first_line.len());
                        let end = Location::new(endl, endc);
                        diagnostic.amend(Fix::deletion(start, end));
                        break;
                    }
                    endl += 1;
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
                violations::MultiLineSummarySecondLine,
                Range::from_located(docstring.expr),
            );
            if checker.patch(diagnostic.kind.rule()) &&
                // Skip cases where leading quote is preceded by non-whitespace for now,
                // e.g. `def foo(): """...`. Figuring out correct indentation after the newline
                // we'd add would require some work.
                docstring.indentation.chars().all(char::is_whitespace)
            {
                let loc = docstring.expr.location;
                let prefix = leading_quote(contents).unwrap();
                // Use replacement instead of insert to trim possible whitespace between leading
                // quote and text.
                let repl = format!(
                    "\n{}{}",
                    docstring.indentation,
                    first_line.strip_prefix(prefix).unwrap().trim_start()
                );
                diagnostic.amend(Fix::replacement(
                    repl,
                    Location::new(loc.row(), loc.column() + prefix.len()),
                    Location::new(loc.row(), loc.column() + first_line.len()),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
