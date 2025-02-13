//! Lint rules based on checking physical lines.

use ruff_diagnostics::Diagnostic;
use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_source_file::UniversalNewlines;
use ruff_text_size::TextSize;

use crate::registry::Rule;
use crate::rules::flake8_copyright::rules::missing_copyright_notice;
use crate::rules::pycodestyle::rules::{
    doc_line_too_long, line_too_long, mixed_spaces_and_tabs, no_newline_at_end_of_file,
    trailing_whitespace,
};
use crate::rules::pylint;
use crate::rules::ruff::rules::indented_form_feed;
use crate::settings::LinterSettings;
use crate::Locator;

pub(crate) fn check_physical_lines(
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    doc_lines: &[TextSize],
    settings: &LinterSettings,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let enforce_doc_line_too_long = settings.rules.enabled(Rule::DocLineTooLong);
    let enforce_line_too_long = settings.rules.enabled(Rule::LineTooLong);
    let enforce_no_newline_at_end_of_file = settings.rules.enabled(Rule::MissingNewlineAtEndOfFile);
    let enforce_mixed_spaces_and_tabs = settings.rules.enabled(Rule::MixedSpacesAndTabs);
    let enforce_bidirectional_unicode = settings.rules.enabled(Rule::BidirectionalUnicode);
    let enforce_trailing_whitespace = settings.rules.enabled(Rule::TrailingWhitespace);
    let enforce_blank_line_contains_whitespace =
        settings.rules.enabled(Rule::BlankLineWithWhitespace);
    let enforce_copyright_notice = settings.rules.enabled(Rule::MissingCopyrightNotice);

    let mut doc_lines_iter = doc_lines.iter().peekable();
    let comment_ranges = indexer.comment_ranges();

    for line in locator.contents().universal_newlines() {
        while doc_lines_iter
            .next_if(|doc_line_start| line.range().contains_inclusive(**doc_line_start))
            .is_some()
        {
            if enforce_doc_line_too_long {
                if let Some(diagnostic) = doc_line_too_long(&line, comment_ranges, settings) {
                    diagnostics.push(diagnostic);
                }
            }
        }

        if enforce_mixed_spaces_and_tabs {
            if let Some(diagnostic) = mixed_spaces_and_tabs(&line) {
                diagnostics.push(diagnostic);
            }
        }

        if enforce_line_too_long {
            if let Some(diagnostic) = line_too_long(&line, comment_ranges, settings) {
                diagnostics.push(diagnostic);
            }
        }

        if enforce_bidirectional_unicode {
            diagnostics.extend(pylint::rules::bidirectional_unicode(&line));
        }

        if enforce_trailing_whitespace || enforce_blank_line_contains_whitespace {
            if let Some(diagnostic) = trailing_whitespace(&line, locator, indexer, settings) {
                diagnostics.push(diagnostic);
            }
        }

        if settings.rules.enabled(Rule::IndentedFormFeed) {
            if let Some(diagnostic) = indented_form_feed(&line) {
                diagnostics.push(diagnostic);
            }
        }
    }

    if enforce_no_newline_at_end_of_file {
        if let Some(diagnostic) = no_newline_at_end_of_file(locator, stylist) {
            diagnostics.push(diagnostic);
        }
    }

    if enforce_copyright_notice {
        if let Some(diagnostic) = missing_copyright_notice(locator, settings) {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use ruff_python_codegen::Stylist;
    use ruff_python_index::Indexer;
    use ruff_python_parser::parse_module;

    use crate::line_width::LineLength;
    use crate::registry::Rule;
    use crate::rules::pycodestyle;
    use crate::settings::LinterSettings;
    use crate::Locator;

    use super::check_physical_lines;

    #[test]
    fn e501_non_ascii_char() {
        let line = "'\u{4e9c}' * 2"; // 7 in UTF-32, 9 in UTF-8.
        let locator = Locator::new(line);
        let parsed = parse_module(line).unwrap();
        let indexer = Indexer::from_tokens(parsed.tokens(), locator.contents());
        let stylist = Stylist::from_tokens(parsed.tokens(), locator.contents());

        let check_with_max_line_length = |line_length: LineLength| {
            check_physical_lines(
                &locator,
                &stylist,
                &indexer,
                &[],
                &LinterSettings {
                    pycodestyle: pycodestyle::settings::Settings {
                        max_line_length: line_length,
                        ..pycodestyle::settings::Settings::default()
                    },
                    ..LinterSettings::for_rule(Rule::LineTooLong)
                },
            )
        };
        let line_length = LineLength::try_from(8).unwrap();
        assert_eq!(check_with_max_line_length(line_length), vec![]);
        assert_eq!(check_with_max_line_length(line_length), vec![]);
    }
}
