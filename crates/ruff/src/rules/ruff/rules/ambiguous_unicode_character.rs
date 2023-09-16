use std::fmt;

use bitflags::bitflags;

use ruff_diagnostics::{Diagnostic, DiagnosticKind, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_source_file::Locator;
use ruff_text_size::{TextLen, TextRange, TextSize};

use crate::registry::AsRule;
use crate::rules::ruff::rules::confusables::confusable;
use crate::rules::ruff::rules::Context;
use crate::settings::Settings;

/// ## What it does
/// Checks for ambiguous unicode characters in strings.
///
/// ## Why is this bad?
/// The use of ambiguous unicode characters can confuse readers and cause
/// subtle bugs.
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
#[violation]
pub struct AmbiguousUnicodeCharacterString {
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
/// Checks for ambiguous unicode characters in docstrings.
///
/// ## Why is this bad?
/// The use of ambiguous unicode characters can confuse readers and cause
/// subtle bugs.
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
#[violation]
pub struct AmbiguousUnicodeCharacterDocstring {
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
/// Checks for ambiguous unicode characters in comments.
///
/// ## Why is this bad?
/// The use of ambiguous unicode characters can confuse readers and cause
/// subtle bugs.
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
#[violation]
pub struct AmbiguousUnicodeCharacterComment {
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

pub(crate) fn ambiguous_unicode_character(
    diagnostics: &mut Vec<Diagnostic>,
    locator: &Locator,
    range: TextRange,
    context: Context,
    settings: &Settings,
) {
    let text = locator.slice(range);

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
                if let Some(representant) = confusable(current_char as u32) {
                    let candidate = Candidate::new(
                        TextSize::try_from(relative_offset).unwrap() + range.start(),
                        current_char,
                        representant as char,
                    );
                    if let Some(diagnostic) = candidate.into_diagnostic(context, settings) {
                        diagnostics.push(diagnostic);
                    }
                }
            }
        } else if current_char.is_ascii() {
            // The current word contains at least one ASCII character.
            word_flags |= WordFlags::ASCII;
        } else if let Some(representant) = confusable(current_char as u32) {
            // The current word contains an ambiguous unicode character.
            word_candidates.push(Candidate::new(
                TextSize::try_from(relative_offset).unwrap() + range.start(),
                current_char,
                representant as char,
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
        const ASCII = 0b0000_0001;
        /// The word contains at least one unambiguous unicode character (like `β`).
        const UNAMBIGUOUS_UNICODE = 0b0000_0010;
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

    fn into_diagnostic(self, context: Context, settings: &Settings) -> Option<Diagnostic> {
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
