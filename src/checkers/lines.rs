//! Lint rules based on checking raw physical lines.

use crate::registry::{Diagnostic, Rule};
use crate::rules::pycodestyle::rules::{
    doc_line_too_long, line_too_long, no_newline_at_end_of_file,
};
use crate::rules::pygrep_hooks::rules::{blanket_noqa, blanket_type_ignore};
use crate::rules::pyupgrade::rules::unnecessary_coding_comment;
use crate::settings::{flags, Settings};

pub fn check_lines(
    contents: &str,
    commented_lines: &[usize],
    doc_lines: &[usize],
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = vec![];

    let enforce_blanket_noqa = settings.rules.enabled(&Rule::BlanketNOQA);
    let enforce_blanket_type_ignore = settings.rules.enabled(&Rule::BlanketTypeIgnore);
    let enforce_doc_line_too_long = settings.rules.enabled(&Rule::DocLineTooLong);
    let enforce_line_too_long = settings.rules.enabled(&Rule::LineTooLong);
    let enforce_no_newline_at_end_of_file = settings.rules.enabled(&Rule::NoNewLineAtEndOfFile);
    let enforce_unnecessary_coding_comment = settings
        .rules
        .enabled(&Rule::PEP3120UnnecessaryCodingComment);

    let mut commented_lines_iter = commented_lines.iter().peekable();
    let mut doc_lines_iter = doc_lines.iter().peekable();
    for (index, line) in contents.lines().enumerate() {
        while commented_lines_iter
            .next_if(|lineno| &(index + 1) == *lineno)
            .is_some()
        {
            if enforce_unnecessary_coding_comment {
                if index < 2 {
                    if let Some(diagnostic) = unnecessary_coding_comment(
                        index,
                        line,
                        matches!(autofix, flags::Autofix::Enabled)
                            && settings
                                .rules
                                .should_fix(&Rule::PEP3120UnnecessaryCodingComment),
                    ) {
                        diagnostics.push(diagnostic);
                    }
                }
            }

            if enforce_blanket_type_ignore {
                if let Some(diagnostic) = blanket_type_ignore(index, line) {
                    diagnostics.push(diagnostic);
                }
            }

            if enforce_blanket_noqa {
                if let Some(diagnostic) = blanket_noqa(index, line) {
                    diagnostics.push(diagnostic);
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

        if enforce_line_too_long {
            if let Some(diagnostic) = line_too_long(index, line, settings) {
                diagnostics.push(diagnostic);
            }
        }
    }

    if enforce_no_newline_at_end_of_file {
        if let Some(diagnostic) = no_newline_at_end_of_file(
            contents,
            matches!(autofix, flags::Autofix::Enabled)
                && settings.rules.should_fix(&Rule::NoNewLineAtEndOfFile),
        ) {
            diagnostics.push(diagnostic);
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {

    use super::check_lines;
    use crate::registry::Rule;
    use crate::settings::{flags, Settings};

    #[test]
    fn e501_non_ascii_char() {
        let line = "'\u{4e9c}' * 2"; // 7 in UTF-32, 9 in UTF-8.
        let check_with_max_line_length = |line_length: usize| {
            check_lines(
                line,
                &[],
                &[],
                &Settings {
                    line_length,
                    ..Settings::for_rule(Rule::LineTooLong)
                },
                flags::Autofix::Enabled,
            )
        };
        assert!(!check_with_max_line_length(6).is_empty());
        assert!(check_with_max_line_length(7).is_empty());
    }
}
