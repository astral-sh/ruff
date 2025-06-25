//! Lint rules based on checking physical lines.

use ruff_python_codegen::Stylist;
use ruff_python_index::Indexer;
use ruff_source_file::UniversalNewlines;
use ruff_text_size::TextSize;

use crate::Locator;
use crate::registry::Rule;
use crate::rules::flake8_copyright::rules::missing_copyright_notice;
use crate::rules::pycodestyle::rules::{
    doc_line_too_long, line_too_long, mixed_spaces_and_tabs, no_newline_at_end_of_file,
    trailing_whitespace,
};
use crate::rules::pylint;
use crate::rules::ruff::rules::indented_form_feed;
use crate::settings::LinterSettings;

use super::ast::LintContext;

pub(crate) fn check_physical_lines(
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    doc_lines: &[TextSize],
    settings: &LinterSettings,
    context: &LintContext,
) {
    let enforce_doc_line_too_long = context.is_rule_enabled(Rule::DocLineTooLong);
    let enforce_line_too_long = context.is_rule_enabled(Rule::LineTooLong);
    let enforce_no_newline_at_end_of_file =
        context.is_rule_enabled(Rule::MissingNewlineAtEndOfFile);
    let enforce_mixed_spaces_and_tabs = context.is_rule_enabled(Rule::MixedSpacesAndTabs);
    let enforce_bidirectional_unicode = context.is_rule_enabled(Rule::BidirectionalUnicode);
    let enforce_trailing_whitespace = context.is_rule_enabled(Rule::TrailingWhitespace);
    let enforce_blank_line_contains_whitespace =
        context.is_rule_enabled(Rule::BlankLineWithWhitespace);
    let enforce_copyright_notice = context.is_rule_enabled(Rule::MissingCopyrightNotice);

    let mut doc_lines_iter = doc_lines.iter().peekable();
    let comment_ranges = indexer.comment_ranges();

    for line in locator.contents().universal_newlines() {
        while doc_lines_iter
            .next_if(|doc_line_start| line.range().contains_inclusive(**doc_line_start))
            .is_some()
        {
            if enforce_doc_line_too_long {
                doc_line_too_long(&line, comment_ranges, settings, context);
            }
        }

        if enforce_mixed_spaces_and_tabs {
            mixed_spaces_and_tabs(&line, context);
        }

        if enforce_line_too_long {
            line_too_long(&line, comment_ranges, settings, context);
        }

        if enforce_bidirectional_unicode {
            pylint::rules::bidirectional_unicode(&line, context);
        }

        if enforce_trailing_whitespace || enforce_blank_line_contains_whitespace {
            trailing_whitespace(&line, locator, indexer, context);
        }

        if context.is_rule_enabled(Rule::IndentedFormFeed) {
            indented_form_feed(&line, context);
        }
    }

    if enforce_no_newline_at_end_of_file {
        no_newline_at_end_of_file(locator, stylist, context);
    }

    if enforce_copyright_notice {
        missing_copyright_notice(locator, settings, context);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ruff_python_codegen::Stylist;
    use ruff_python_index::Indexer;
    use ruff_python_parser::parse_module;

    use crate::Locator;
    use crate::checkers::ast::LintContext;
    use crate::line_width::LineLength;
    use crate::registry::Rule;
    use crate::rules::pycodestyle;
    use crate::settings::LinterSettings;

    use super::check_physical_lines;

    #[test]
    fn e501_non_ascii_char() {
        let line = "'\u{4e9c}' * 2"; // 7 in UTF-32, 9 in UTF-8.
        let locator = Locator::new(line);
        let parsed = parse_module(line).unwrap();
        let indexer = Indexer::from_tokens(parsed.tokens(), locator.contents());
        let stylist = Stylist::from_tokens(parsed.tokens(), locator.contents());

        let check_with_max_line_length = |line_length: LineLength| {
            let settings = LinterSettings {
                pycodestyle: pycodestyle::settings::Settings {
                    max_line_length: line_length,
                    ..pycodestyle::settings::Settings::default()
                },
                ..LinterSettings::for_rule(Rule::LineTooLong)
            };
            let diagnostics = LintContext::new(Path::new("<filename>"), line, &settings);
            check_physical_lines(&locator, &stylist, &indexer, &[], &settings, &diagnostics);
            diagnostics.into_parts().0
        };
        let line_length = LineLength::try_from(8).unwrap();
        assert_eq!(check_with_max_line_length(line_length), vec![]);
        assert_eq!(check_with_max_line_length(line_length), vec![]);
    }
}
