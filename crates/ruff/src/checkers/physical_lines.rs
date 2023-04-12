//! Lint rules based on checking physical lines.

use std::path::Path;

use ruff_diagnostics::Diagnostic;
use ruff_python_ast::newlines::StrExt;
use ruff_python_ast::source_code::{Indexer, Locator, Stylist};

use crate::registry::Rule;
use crate::rules::flake8_executable::helpers::{extract_shebang, ShebangDirective};
use crate::rules::flake8_executable::rules::{
    shebang_missing, shebang_newline, shebang_not_executable, shebang_python, shebang_whitespace,
};
use crate::rules::pycodestyle::rules::{
    doc_line_too_long, line_too_long, mixed_spaces_and_tabs, no_newline_at_end_of_file,
    tab_indentation, trailing_whitespace,
};
use crate::rules::pygrep_hooks::rules::{blanket_noqa, blanket_type_ignore};
use crate::rules::pylint;
use crate::rules::pyupgrade::rules::unnecessary_coding_comment;
use crate::settings::{flags, Settings};

pub fn check_physical_lines(
    path: &Path,
    locator: &Locator,
    stylist: &Stylist,
    indexer: &Indexer,
    doc_lines: &[usize],
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];
    let mut has_any_shebang = false;

    let enforce_blanket_noqa = settings.rules.enabled(Rule::BlanketNOQA);
    let enforce_shebang_not_executable = settings.rules.enabled(Rule::ShebangNotExecutable);
    let enforce_shebang_missing = settings.rules.enabled(Rule::ShebangMissingExecutableFile);
    let enforce_shebang_whitespace = settings.rules.enabled(Rule::ShebangLeadingWhitespace);
    let enforce_shebang_newline = settings.rules.enabled(Rule::ShebangNotFirstLine);
    let enforce_shebang_python = settings.rules.enabled(Rule::ShebangMissingPython);
    let enforce_blanket_type_ignore = settings.rules.enabled(Rule::BlanketTypeIgnore);
    let enforce_doc_line_too_long = settings.rules.enabled(Rule::DocLineTooLong);
    let enforce_line_too_long = settings.rules.enabled(Rule::LineTooLong);
    let enforce_no_newline_at_end_of_file = settings.rules.enabled(Rule::MissingNewlineAtEndOfFile);
    let enforce_unnecessary_coding_comment = settings.rules.enabled(Rule::UTF8EncodingDeclaration);
    let enforce_mixed_spaces_and_tabs = settings.rules.enabled(Rule::MixedSpacesAndTabs);
    let enforce_bidirectional_unicode = settings.rules.enabled(Rule::BidirectionalUnicode);
    let enforce_trailing_whitespace = settings.rules.enabled(Rule::TrailingWhitespace);
    let enforce_blank_line_contains_whitespace =
        settings.rules.enabled(Rule::BlankLineWithWhitespace);
    let enforce_tab_indentation = settings.rules.enabled(Rule::TabIndentation);

    let fix_unnecessary_coding_comment =
        autofix.into() && settings.rules.should_fix(Rule::UTF8EncodingDeclaration);
    let fix_shebang_whitespace =
        autofix.into() && settings.rules.should_fix(Rule::ShebangLeadingWhitespace);

    let mut commented_lines_iter = indexer.commented_lines().iter().peekable();
    let mut doc_lines_iter = doc_lines.iter().peekable();

    let string_lines = indexer.string_ranges();

    for (index, line) in locator.contents().universal_newlines().enumerate() {
        while commented_lines_iter
            .next_if(|lineno| &(index + 1) == *lineno)
            .is_some()
        {
            if enforce_unnecessary_coding_comment {
                if index < 2 {
                    if let Some(diagnostic) =
                        unnecessary_coding_comment(index, line, fix_unnecessary_coding_comment)
                    {
                        diagnostics.push(diagnostic);
                    }
                }
            }

            if enforce_blanket_type_ignore {
                blanket_type_ignore(&mut diagnostics, index, line);
            }

            if enforce_blanket_noqa {
                blanket_noqa(&mut diagnostics, index, line);
            }

            if enforce_shebang_missing
                || enforce_shebang_not_executable
                || enforce_shebang_whitespace
                || enforce_shebang_newline
                || enforce_shebang_python
            {
                let shebang = extract_shebang(line);
                if enforce_shebang_not_executable {
                    if let Some(diagnostic) = shebang_not_executable(path, index, &shebang) {
                        diagnostics.push(diagnostic);
                    }
                }
                if enforce_shebang_missing {
                    if !has_any_shebang && matches!(shebang, ShebangDirective::Match(_, _, _, _)) {
                        has_any_shebang = true;
                    }
                }
                if enforce_shebang_whitespace {
                    if let Some(diagnostic) =
                        shebang_whitespace(index, &shebang, fix_shebang_whitespace)
                    {
                        diagnostics.push(diagnostic);
                    }
                }
                if enforce_shebang_newline {
                    if let Some(diagnostic) = shebang_newline(index, &shebang) {
                        diagnostics.push(diagnostic);
                    }
                }
                if enforce_shebang_python {
                    if let Some(diagnostic) = shebang_python(index, &shebang) {
                        diagnostics.push(diagnostic);
                    }
                }
            }
        }

        while doc_lines_iter
            .next_if(|lineno| &(index + 1) == *lineno)
            .is_some()
        {
            if enforce_doc_line_too_long {
                if let Some(diagnostic) = doc_line_too_long(index, line, settings) {
                    diagnostics.push(diagnostic);
                }
            }
        }

        if enforce_mixed_spaces_and_tabs {
            if let Some(diagnostic) = mixed_spaces_and_tabs(index, line) {
                diagnostics.push(diagnostic);
            }
        }

        if enforce_line_too_long {
            if let Some(diagnostic) = line_too_long(index, line, settings) {
                diagnostics.push(diagnostic);
            }
        }

        if enforce_bidirectional_unicode {
            diagnostics.extend(pylint::rules::bidirectional_unicode(index, line));
        }

        if enforce_trailing_whitespace || enforce_blank_line_contains_whitespace {
            if let Some(diagnostic) = trailing_whitespace(index, line, settings, autofix) {
                diagnostics.push(diagnostic);
            }
        }

        if enforce_tab_indentation {
            if let Some(diagnostic) = tab_indentation(index + 1, line, string_lines) {
                diagnostics.push(diagnostic);
            }
        }
    }

    if enforce_no_newline_at_end_of_file {
        if let Some(diagnostic) = no_newline_at_end_of_file(
            locator,
            stylist,
            autofix.into() && settings.rules.should_fix(Rule::MissingNewlineAtEndOfFile),
        ) {
            diagnostics.push(diagnostic);
        }
    }

    if enforce_shebang_missing && !has_any_shebang {
        if let Some(diagnostic) = shebang_missing(path) {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use rustpython_parser::lexer::lex;
    use rustpython_parser::Mode;
    use std::path::Path;

    use ruff_python_ast::source_code::{Indexer, Locator, Stylist};

    use crate::registry::Rule;
    use crate::settings::{flags, Settings};

    use super::check_physical_lines;

    #[test]
    fn e501_non_ascii_char() {
        let line = "'\u{4e9c}' * 2"; // 7 in UTF-32, 9 in UTF-8.
        let locator = Locator::new(line);
        let tokens: Vec<_> = lex(line, Mode::Module).collect();
        let indexer: Indexer = tokens.as_slice().into();
        let stylist = Stylist::from_tokens(&tokens, &locator);

        let check_with_max_line_length = |line_length: usize| {
            check_physical_lines(
                Path::new("foo.py"),
                &locator,
                &stylist,
                &indexer,
                &[],
                &Settings {
                    line_length,
                    ..Settings::for_rule(Rule::LineTooLong)
                },
                flags::Autofix::Enabled,
            )
        };
        assert_eq!(check_with_max_line_length(8), vec![]);
        assert_eq!(check_with_max_line_length(8), vec![]);
    }
}
