use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_semantic::analyze::visibility::is_property;
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{Ranged, TextLen, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::docstrings::Docstring;
use crate::rules::pydocstyle::helpers::normalize_word;
use crate::rules::pydocstyle::settings::Settings;

/// ## What it does
/// Checks for `@property` method docstrings that start with known verbs
/// (e.g., "returns", "gets", etc).
///
/// ## Why is this bad?
/// The [Google Python style guide] recommends that the docstring for a
/// `@property` data descriptor use the same style as the docstring for an
/// attribute or a function argument (e.g., `"""The Bigtable path."""`),
/// rather than a function-style docstring (e.g.,
/// `"""Returns the Bigtable path."""`).
///
/// This rule is only enforced when using the `google` convention.
///
/// ## Example
/// ```python
/// class Foo:
///     @property
///     def bar(self) -> str:
///         """Returns the bar."""
///         return self._bar
/// ```
///
/// Use instead:
/// ```python
/// class Foo:
///     @property
///     def bar(self) -> str:
///         """The bar."""
///         return self._bar
/// ```
///
/// ## Options
/// - `lint.pydocstyle.convention`
/// - `lint.pydocstyle.property-decorators`
///
/// ## References
/// - [Google Python Style Guide – Properties](https://google.github.io/styleguide/pyguide.html#383-functions-and-methods)
///
/// [Google Python style guide]: https://google.github.io/styleguide/pyguide.html#383-functions-and-methods
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct PropertyDocstringStartsWithVerb {
    pub(crate) first_word: String,
}

impl Violation for PropertyDocstringStartsWithVerb {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PropertyDocstringStartsWithVerb { first_word } = self;
        format!(
            r#"Property docstring should not start with a verb ("{first_word}")"#
        )
    }
}

const DISALLOWED_VERBS: &[&str] = &[
    "return",
    "returns",
    "get",
    "gets",
    "yield",
    "yields",
    "fetch",
    "fetches",
    "retrieve",
    "retrieves",
];

/// D421
pub(crate) fn property_docstring_verb(
    checker: &Checker,
    docstring: &Docstring,
    settings: &Settings,
) {
    let Some(function) = docstring.definition.as_function_def() else {
        return;
    };

    if !is_property(
        &function.decorator_list,
        settings.property_decorators(),
        checker.semantic(),
    ) {
        return;
    }

    let body = docstring.body();
    let trim_start_body = body.trim_start();

    if let Some(first_line) = trim_start_body.universal_newlines().next()
        && let Some(first_word) = first_line.as_str().split_whitespace().next()
        && let first_word_norm = normalize_word(first_word)
        && !first_word_norm.is_empty()
        && DISALLOWED_VERBS.contains(&first_word_norm.as_str())
    {
        let leading_whitespace_len = body.text_len() - trim_start_body.text_len();
        checker.report_diagnostic(
            PropertyDocstringStartsWithVerb {
                first_word: first_word.to_string(),
            },
            TextRange::at(body.start() + leading_whitespace_len, first_word.text_len()),
        );
    }
}
