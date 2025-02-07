use std::sync::LazyLock;

use imperative::Mood;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::analyze::visibility::{is_property, is_test};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::rules::pydocstyle::helpers::normalize_word;
use crate::rules::pydocstyle::settings::Settings;

static MOOD: LazyLock<Mood> = LazyLock::new(Mood::new);

/// ## What it does
/// Checks for docstring first lines that are not in an imperative mood.
///
/// ## Why is this bad?
/// [PEP 257] recommends that the first line of a docstring be written in the
/// imperative mood, for consistency.
///
/// Hint: to rewrite the docstring in the imperative, phrase the first line as
/// if it were a command.
///
/// This rule may not apply to all projects; its applicability is a matter of
/// convention. By default, this rule is enabled when using the `numpy` and
/// `pep257` conventions, and disabled when using the `google` conventions.
///
/// ## Example
/// ```python
/// def average(values: list[float]) -> float:
///     """Returns the mean of the given values."""
/// ```
///
/// Use instead:
/// ```python
/// def average(values: list[float]) -> float:
///     """Return the mean of the given values."""
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
/// - `lint.pydocstyle.property-decorators`
/// - `lint.pydocstyle.ignore-decorators`
///
/// ## References
/// - [PEP 257 – Docstring Conventions](https://peps.python.org/pep-0257/)
///
/// [PEP 257]: https://peps.python.org/pep-0257/
#[derive(ViolationMetadata)]
pub(crate) struct NonImperativeMood {
    first_line: String,
}

impl Violation for NonImperativeMood {
    #[derive_message_formats]
    fn message(&self) -> String {
        let NonImperativeMood { first_line } = self;
        format!("First line of docstring should be in imperative mood: \"{first_line}\"")
    }
}

/// D401
pub(crate) fn non_imperative_mood(checker: &Checker, docstring: &Docstring, settings: &Settings) {
    let Some(function) = docstring.definition.as_function_def() else {
        return;
    };

    if is_test(&function.name) {
        return;
    }

    if is_property(
        &function.decorator_list,
        settings.property_decorators(),
        checker.semantic(),
    ) {
        return;
    }

    let body = docstring.body();

    // Find first line, disregarding whitespace.
    let first_line = match body.trim().universal_newlines().next() {
        Some(line) => line.as_str().trim(),
        None => return,
    };

    // Find the first word on that line and normalize it to lower-case.
    let first_word_norm = match first_line.split_whitespace().next() {
        Some(word) => normalize_word(word),
        None => return,
    };
    if first_word_norm.is_empty() {
        return;
    }

    if matches!(MOOD.is_imperative(&first_word_norm), Some(false)) {
        checker.report_diagnostic(Diagnostic::new(
            NonImperativeMood {
                first_line: first_line.to_string(),
            },
            docstring.range(),
        ));
    }
}
