use std::fmt;

use bitflags::bitflags;

use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, StringLike};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::ruff::rules::confusables::confusable;
use crate::rules::ruff::rules::Context;
use crate::settings::LinterSettings;
use crate::Locator;

/// ## What it does
/// Checks for ambiguous Unicode characters in strings.
///
/// ## Why is this bad?
/// Some Unicode characters are visually similar to ASCII characters, but have
/// different code points. For example, `GREEK CAPITAL LETTER ALPHA` (`U+0391`)
/// is visually similar, but not identical, to the ASCII character `A`.
///
/// The use of ambiguous Unicode characters can confuse readers, cause subtle
/// bugs, and even make malicious code look harmless.
///
/// In [preview], this rule will also flag Unicode characters that are
/// confusable with other, non-preferred Unicode characters. For example, the
/// spec recommends `GREEK CAPITAL LETTER OMEGA` over `OHM SIGN`.
///
/// You can omit characters from being flagged as ambiguous via the
/// [`lint.allowed-confusables`] setting.
///
/// ## Example
/// ```python
/// print("Ηello, world!")  # "Η" is the Greek eta (`U+0397`).
/// ```
///
/// Use instead:
/// ```python
/// print("Hello, world!")  # "H" is the Latin capital H (`U+0048`).
/// ```
///
/// ## Options
/// - `lint.allowed-confusables`
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct AmbiguousUnicodeCharacterString {
    confusable: char,
    representant: char,
}

impl Violation for AmbiguousUnicodeCharacterString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterString {
            confusable,
            representant,
        } = self;
        format!(
            "String contains ambiguous {}. Did you mean {}?",
            NamedUnicode(*confusable),
            NamedUnicode(*representant)
        )
    }
}

/// ## What it does
/// Checks for ambiguous Unicode characters in docstrings.
///
/// ## Why is this bad?
/// Some Unicode characters are visually similar to ASCII characters, but have
/// different code points. For example, `GREEK CAPITAL LETTER ALPHA` (`U+0391`)
/// is visually similar, but not identical, to the ASCII character `A`.
///
/// The use of ambiguous Unicode characters can confuse readers, cause subtle
/// bugs, and even make malicious code look harmless.
///
/// In [preview], this rule will also flag Unicode characters that are
/// confusable with other, non-preferred Unicode characters. For example, the
/// spec recommends `GREEK CAPITAL LETTER OMEGA` over `OHM SIGN`.
///
/// You can omit characters from being flagged as ambiguous via the
/// [`lint.allowed-confusables`] setting.
///
/// ## Example
/// ```python
/// """A lovely docstring (with a `U+FF09` parenthesis）."""
/// ```
///
/// Use instead:
/// ```python
/// """A lovely docstring (with no strange parentheses)."""
/// ```
///
/// ## Options
/// - `lint.allowed-confusables`
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct AmbiguousUnicodeCharacterDocstring {
    confusable: char,
    representant: char,
}

impl Violation for AmbiguousUnicodeCharacterDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterDocstring {
            confusable,
            representant,
        } = self;
        format!(
            "Docstring contains ambiguous {}. Did you mean {}?",
            NamedUnicode(*confusable),
            NamedUnicode(*representant)
        )
    }
}

/// ## What it does
/// Checks for ambiguous Unicode characters in comments.
///
/// ## Why is this bad?
/// Some Unicode characters are visually similar to ASCII characters, but have
/// different code points. For example, `GREEK CAPITAL LETTER ALPHA` (`U+0391`)
/// is visually similar, but not identical, to the ASCII character `A`.
///
/// The use of ambiguous Unicode characters can confuse readers, cause subtle
/// bugs, and even make malicious code look harmless.
///
/// In [preview], this rule will also flag Unicode characters that are
/// confusable with other, non-preferred Unicode characters. For example, the
/// spec recommends `GREEK CAPITAL LETTER OMEGA` over `OHM SIGN`.
///
/// You can omit characters from being flagged as ambiguous via the
/// [`lint.allowed-confusables`] setting.
///
/// ## Example
/// ```python
/// foo()  # nоqa  # "о" is Cyrillic (`U+043E`)
/// ```
///
/// Use instead:
/// ```python
/// foo()  # noqa  # "o" is Latin (`U+006F`)
/// ```
///
/// ## Options
/// - `lint.allowed-confusables`
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct AmbiguousUnicodeCharacterComment {
    confusable: char,
    representant: char,
}

impl Violation for AmbiguousUnicodeCharacterComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterComment {
            confusable,
            representant,
        } = self;
        format!(
            "Comment contains ambiguous {}. Did you mean {}?",
            NamedUnicode(*confusable),
            NamedUnicode(*representant)
        )
    }
}

/// RUF003
pub(crate) fn ambiguous_unicode_character_comment(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    range: TextRange,
    settings: &LinterSettings,
) {
    let text = locator.slice(range);
    ambiguous_unicode_character(diagnostics, text, range, Context::Comment, settings);
}

/// RUF001, RUF002
pub(crate) fn ambiguous_unicode_character_string(checker: &Checker, string_like: StringLike) {
    let semantic = checker.semantic();

    if semantic.in_string_type_definition() {
        return;
    }

    let context = if semantic.in_pep_257_docstring() {
        Context::Docstring
    } else {
        Context::String
    };

    for part in string_like.parts() {
        match part {
            ast::StringLikePart::String(string_literal) => {
                let text = checker.locator().slice(string_literal);
                let mut diagnostics = Vec::new();
                ambiguous_unicode_character(
                    &mut diagnostics,
                    text,
                    string_literal.range(),
                    context,
                    checker.settings,
                );
                checker.report_diagnostics(diagnostics);
            }
            ast::StringLikePart::Bytes(_) => {}
            ast::StringLikePart::FString(f_string) => {
                for literal in f_string.elements.literals() {
                    let text = checker.locator().slice(literal);
                    let mut diagnostics = Vec::new();
                    ambiguous_unicode_character(
                        &mut diagnostics,
                        text,
                        literal.range(),
                        context,
                        checker.settings,
                    );
                    checker.report_diagnostics(diagnostics);
                }
            }
        }
    }
}

fn ambiguous_unicode_character(
    diagnostics: &mut Vec<Diagnostic>,
    text: &str,
    range: TextRange,
    context: Context,
    settings: &LinterSettings,
) {
    // Most of the time, we don't need to check for ambiguous unicode characters at all.
    if text.is_ascii() {
        return;
    }

    // Iterate over the "words" in the text.
    let mut word_flags = WordFlags::empty();
    let mut word_candidates: Vec<Candidate> = vec![];
    for (relative_offset, current_char) in text.char_indices() {
        // Word boundary.
        if !current_char.is_alphanumeric() {
            if !word_candidates.is_empty() {
                if word_flags.is_candidate_word() {
                    for candidate in word_candidates.drain(..) {
                        if let Some(diagnostic) = candidate.into_diagnostic(context, settings) {
                            diagnostics.push(diagnostic);
                        }
                    }
                }
                word_candidates.clear();
            }
            word_flags = WordFlags::empty();

            // Check if the boundary character is itself an ambiguous unicode character, in which
            // case, it's always included as a diagnostic.
            if !current_char.is_ascii() {
                if let Some(representant) = confusable(current_char as u32)
                    .filter(|representant| settings.preview.is_enabled() || representant.is_ascii())
                {
                    let candidate = Candidate::new(
                        TextSize::try_from(relative_offset).unwrap() + range.start(),
                        current_char,
                        representant,
                    );
                    if let Some(diagnostic) = candidate.into_diagnostic(context, settings) {
                        diagnostics.push(diagnostic);
                    }
                }
            }
        } else if current_char.is_ascii() {
            // The current word contains at least one ASCII character.
            word_flags |= WordFlags::ASCII;
        } else if let Some(representant) = confusable(current_char as u32)
            .filter(|representant| settings.preview.is_enabled() || representant.is_ascii())
        {
            // The current word contains an ambiguous unicode character.
            word_candidates.push(Candidate::new(
                TextSize::try_from(relative_offset).unwrap() + range.start(),
                current_char,
                representant,
            ));
        } else {
            // The current word contains at least one unambiguous unicode character.
            word_flags |= WordFlags::UNAMBIGUOUS_UNICODE;
        }
    }

    // End of the text.
    if !word_candidates.is_empty() {
        if word_flags.is_candidate_word() {
            for candidate in word_candidates.drain(..) {
                if let Some(diagnostic) = candidate.into_diagnostic(context, settings) {
                    diagnostics.push(diagnostic);
                }
            }
        }
        word_candidates.clear();
    }
}

bitflags! {
    #[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
    pub struct WordFlags: u8 {
        /// The word contains at least one ASCII character (like `B`).
        const ASCII = 1 << 0;
        /// The word contains at least one unambiguous unicode character (like `β`).
        const UNAMBIGUOUS_UNICODE = 1 << 1;
    }
}

impl WordFlags {
    /// Return `true` if the flags indicate that the word is a candidate for flagging
    /// ambiguous unicode characters.
    ///
    /// We follow VS Code's logic for determining whether ambiguous unicode characters within a
    /// given word should be flagged, i.e., we flag a word if it contains at least one ASCII
    /// character, or is purely unicode but _only_ consists of ambiguous characters.
    ///
    /// See: [VS Code](https://github.com/microsoft/vscode/issues/143720#issuecomment-1048757234)
    const fn is_candidate_word(self) -> bool {
        self.contains(WordFlags::ASCII) || !self.contains(WordFlags::UNAMBIGUOUS_UNICODE)
    }
}

/// An ambiguous unicode character in the text.
struct Candidate {
    /// The offset of the candidate in the text.
    offset: TextSize,
    /// The ambiguous unicode character.
    confusable: char,
    /// The character with which the ambiguous unicode character is confusable.
    representant: char,
}

impl Candidate {
    fn new(offset: TextSize, confusable: char, representant: char) -> Self {
        Self {
            offset,
            confusable,
            representant,
        }
    }

    fn into_diagnostic(self, context: Context, settings: &LinterSettings) -> Option<Diagnostic> {
        if !settings.allowed_confusables.contains(&self.confusable) {
            let char_range = TextRange::at(self.offset, self.confusable.text_len());
            let diagnostic = Diagnostic::new::<DiagnosticKind>(
                match context {
                    Context::String => AmbiguousUnicodeCharacterString {
                        confusable: self.confusable,
                        representant: self.representant,
                    }
                    .into(),
                    Context::Docstring => AmbiguousUnicodeCharacterDocstring {
                        confusable: self.confusable,
                        representant: self.representant,
                    }
                    .into(),
                    Context::Comment => AmbiguousUnicodeCharacterComment {
                        confusable: self.confusable,
                        representant: self.representant,
                    }
                    .into(),
                },
                char_range,
            );
            if settings.rules.enabled(diagnostic.kind.rule()) {
                return Some(diagnostic);
            }
        }
        None
    }
}

struct NamedUnicode(char);

impl fmt::Display for NamedUnicode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let NamedUnicode(c) = self;
        if let Some(name) = unicode_names2::name(*c) {
            write!(f, "`{c}` ({name})")
        } else {
            write!(f, "`{c}`")
        }
    }
}
