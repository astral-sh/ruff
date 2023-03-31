use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::newlines::{StrExt, UniversalNewlineIterator};
use ruff_text_size::{TextLen, TextRange};

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::{AsRule, Rule};

#[violation]
pub struct OneBlankLineBeforeClass {
    pub lines: usize,
}

impl AlwaysAutofixableViolation for OneBlankLineBeforeClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("1 blank line required before class docstring")
    }

    fn autofix_title(&self) -> String {
        "Insert 1 blank line before class docstring".to_string()
    }
}

#[violation]
pub struct OneBlankLineAfterClass {
    pub lines: usize,
}

impl AlwaysAutofixableViolation for OneBlankLineAfterClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("1 blank line required after class docstring")
    }

    fn autofix_title(&self) -> String {
        "Insert 1 blank line after class docstring".to_string()
    }
}

#[violation]
pub struct BlankLineBeforeClass {
    pub lines: usize,
}

impl AlwaysAutofixableViolation for BlankLineBeforeClass {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("No blank lines allowed before class docstring")
    }

    fn autofix_title(&self) -> String {
        "Remove blank line(s) before class docstring".to_string()
    }
}

/// D203, D204, D211
pub fn blank_before_after_class(checker: &mut Checker, docstring: &Docstring) {
    let (DefinitionKind::Class(parent) | DefinitionKind::NestedClass(parent)) = &docstring.kind else {
        return;
    };

    if checker
        .settings
        .rules
        .enabled(Rule::OneBlankLineBeforeClass)
        || checker.settings.rules.enabled(Rule::BlankLineBeforeClass)
    {
        let before = checker
            .locator
            .slice(TextRange::new(parent.start(), docstring.start()));

        let mut blank_lines_before = 0usize;
        let mut lines = UniversalNewlineIterator::with_offset(before, parent.start()).rev();
        let mut blank_lines_start = lines.next().map(|line| line.start()).unwrap_or_default();

        for line in lines {
            if line.trim().is_empty() {
                blank_lines_before += 1;
                blank_lines_start = line.start();
            } else {
                break;
            }
        }

        if checker.settings.rules.enabled(Rule::BlankLineBeforeClass) {
            if blank_lines_before != 0 {
                let mut diagnostic = Diagnostic::new(
                    BlankLineBeforeClass {
                        lines: blank_lines_before,
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Delete the blank line before the class.
                    diagnostic.set_fix(Edit::deletion(
                        blank_lines_start,
                        docstring.start() - docstring.indentation.text_len(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        if checker
            .settings
            .rules
            .enabled(Rule::OneBlankLineBeforeClass)
        {
            if blank_lines_before != 1 {
                let mut diagnostic = Diagnostic::new(
                    OneBlankLineBeforeClass {
                        lines: blank_lines_before,
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Insert one blank line before the class.
                    diagnostic.set_fix(Edit::replacement(
                        checker.stylist.line_ending().to_string(),
                        blank_lines_start,
                        docstring.start() - docstring.indentation.text_len(),
                    ));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }

    if checker.settings.rules.enabled(Rule::OneBlankLineAfterClass) {
        let after = checker
            .locator
            .slice(TextRange::new(docstring.end(), parent.end()));

        let all_blank_after = after
            .universal_newlines()
            .skip(1)
            .all(|line| line.trim().is_empty() || line.trim_start().starts_with('#'));
        if all_blank_after {
            return;
        }

        let mut blank_lines_after = 0usize;
        let mut lines = UniversalNewlineIterator::with_offset(after, docstring.end());
        let first_line_start = lines.next().map(|l| l.start()).unwrap_or_default();
        let mut blank_lines_end = docstring.end();

        for line in lines {
            if line.trim().is_empty() {
                blank_lines_end = line.end();
                blank_lines_after += 1;
            } else {
                break;
            }
        }

        if blank_lines_after != 1 {
            let mut diagnostic = Diagnostic::new(
                OneBlankLineAfterClass {
                    lines: blank_lines_after,
                },
                docstring.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                // Insert a blank line before the class (replacing any existing lines).
                diagnostic.set_fix(Edit::replacement(
                    checker.stylist.line_ending().to_string(),
                    first_line_start,
                    blank_lines_end,
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
