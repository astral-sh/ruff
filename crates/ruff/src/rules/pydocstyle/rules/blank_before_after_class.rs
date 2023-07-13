use ruff_text_size::{TextLen, TextRange};
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::{Definition, Member, MemberKind};
use ruff_python_whitespace::{PythonWhitespace, UniversalNewlineIterator, UniversalNewlines};

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::registry::{AsRule, Rule};

/// ## What it does
/// Checks for docstrings on class definitions that are not preceded by a
/// blank line.
///
/// ## Why is this bad?
/// Use a blank line to separate the docstring from the class definition, for
/// consistency.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is disabled when using the `google`,
/// `numpy`, and `pep257` conventions.
///
/// For an alternative, see [D211].
///
/// ## Example
/// ```python
/// class PhotoMetadata:
///     """Metadata about a photo."""
/// ```
///
/// Use instead:
/// ```python
/// class PhotoMetadata:
///
///     """Metadata about a photo."""
/// ```
///
/// ## Options
/// - `pydocstyle.convention`
///
/// [D211]: https://beta.ruff.rs/docs/rules/blank-line-before-class
#[violation]
pub struct OneBlankLineBeforeClass {
    lines: usize,
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

/// ## What it does
/// Checks for class methods that are not separated from the class's docstring
/// by a blank line.
///
/// ## Why is this bad?
/// [PEP 257] recommends the use of a blank line to separate a class's
/// docstring its methods.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is enabled when using the `google`
/// convention, and disabled when using the `numpy` and `pep257` conventions.
///
/// ## Example
/// ```python
/// class PhotoMetadata:
///     """Metadata about a photo."""
///     def __init__(self, file: Path):
///         ...
/// ```
///
/// Use instead:
/// ```python
/// class PhotoMetadata:
///     """Metadata about a photo."""
///
///     def __init__(self, file: Path):
///         ...
/// ```
///
/// ## Options
/// - `pydocstyle.convention`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
/// - [NumPy Style Guide](https://numpydoc.readthedocs.io/en/latest/format.html)
/// - [Google Python Style Guide - Docstrings](https://google.github.io/styleguide/pyguide.html#38-comments-and-docstrings)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[violation]
pub struct OneBlankLineAfterClass {
    lines: usize,
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

/// ## What it does
/// Checks for docstrings on class definitions that are preceded by a blank
/// line.
///
/// ## Why is this bad?
/// Avoid introducing any blank lines between a class definition and its
/// docstring, for consistency.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is enabled when using the `google`,
/// `numpy`, and `pep257` conventions.
///
/// For an alternative, see [D203].
///
/// ## Example
/// ```python
/// class PhotoMetadata:
///     """Metadata about a photo."""
/// ```
///
/// Use instead:
/// ```python
/// class PhotoMetadata:
///
///     """Metadata about a photo."""
/// ```
///
/// ## Options
/// - `pydocstyle.convention`
///
/// [D203]: https://beta.ruff.rs/docs/rules/one-blank-line-before-class
#[violation]
pub struct BlankLineBeforeClass {
    lines: usize,
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
pub(crate) fn blank_before_after_class(checker: &mut Checker, docstring: &Docstring) {
    let Definition::Member(Member {
        kind: MemberKind::Class | MemberKind::NestedClass,
        stmt,
        ..
    }) = docstring.definition
    else {
        return;
    };

    if checker.enabled(Rule::OneBlankLineBeforeClass) || checker.enabled(Rule::BlankLineBeforeClass)
    {
        let before = checker
            .locator
            .slice(TextRange::new(stmt.start(), docstring.start()));

        let mut blank_lines_before = 0usize;
        let mut lines = UniversalNewlineIterator::with_offset(before, stmt.start()).rev();
        let mut blank_lines_start = lines.next().map(|line| line.start()).unwrap_or_default();

        for line in lines {
            if line.trim().is_empty() {
                blank_lines_before += 1;
                blank_lines_start = line.start();
            } else {
                break;
            }
        }

        if checker.enabled(Rule::BlankLineBeforeClass) {
            if blank_lines_before != 0 {
                let mut diagnostic = Diagnostic::new(
                    BlankLineBeforeClass {
                        lines: blank_lines_before,
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Delete the blank line before the class.
                    diagnostic.set_fix(Fix::automatic(Edit::deletion(
                        blank_lines_start,
                        docstring.start() - docstring.indentation.text_len(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
        if checker.enabled(Rule::OneBlankLineBeforeClass) {
            if blank_lines_before != 1 {
                let mut diagnostic = Diagnostic::new(
                    OneBlankLineBeforeClass {
                        lines: blank_lines_before,
                    },
                    docstring.range(),
                );
                if checker.patch(diagnostic.kind.rule()) {
                    // Insert one blank line before the class.
                    diagnostic.set_fix(Fix::automatic(Edit::replacement(
                        checker.stylist.line_ending().to_string(),
                        blank_lines_start,
                        docstring.start() - docstring.indentation.text_len(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }

    if checker.enabled(Rule::OneBlankLineAfterClass) {
        let after = checker
            .locator
            .slice(TextRange::new(docstring.end(), stmt.end()));

        let all_blank_after = after.universal_newlines().skip(1).all(|line| {
            line.trim_whitespace().is_empty() || line.trim_whitespace_start().starts_with('#')
        });
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
                diagnostic.set_fix(Fix::automatic(Edit::replacement(
                    checker.stylist.line_ending().to_string(),
                    first_line_start,
                    blank_lines_end,
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
