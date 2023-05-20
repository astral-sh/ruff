use bitflags::bitflags;
use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, DiagnosticKind, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

use crate::registry::AsRule;
use crate::rules::ruff::rules::confusables::CONFUSABLES;
use crate::rules::ruff::rules::Context;
use crate::settings::Settings;

#[violation]
pub struct AmbiguousUnicodeCharacterString {
    confusable: char,
    representant: char,
}

impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterString {
            confusable,
            representant,
        } = self;
        format!(
            "String contains ambiguous unicode character `{confusable}` (did you mean `{representant}`?)"
        )
    }

    fn autofix_title(&self) -> String {
        let AmbiguousUnicodeCharacterString {
            confusable,
            representant,
        } = self;
        format!("Replace `{confusable}` with `{representant}`")
    }
}

#[violation]
pub struct AmbiguousUnicodeCharacterDocstring {
    confusable: char,
    representant: char,
}

impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterDocstring {
            confusable,
            representant,
        } = self;
        format!(
            "Docstring contains ambiguous unicode character `{confusable}` (did you mean `{representant}`?)"
        )
    }

    fn autofix_title(&self) -> String {
        let AmbiguousUnicodeCharacterDocstring {
            confusable,
            representant,
        } = self;
        format!("Replace `{confusable}` with `{representant}`")
    }
}

#[violation]
pub struct AmbiguousUnicodeCharacterComment {
    confusable: char,
    representant: char,
}

impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterComment {
            confusable,
            representant,
        } = self;
        format!(
            "Comment contains ambiguous unicode character `{confusable}` (did you mean `{representant}`?)"
        )
    }

    fn autofix_title(&self) -> String {
        let AmbiguousUnicodeCharacterComment {
            confusable,
            representant,
        } = self;
        format!("Replace `{confusable}` with `{representant}`")
    }
}

pub(crate) fn ambiguous_unicode_character(
    locator: &Locator,
    range: TextRange,
    context: Context,
    settings: &Settings,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let text = locator.slice(range);

    // Most of the time, we don't need to check for ambiguous unicode characters at all.
    if text.is_ascii() {
        return diagnostics;
    }

    // Iterate over the "words" in the text.
    let mut flags = WordFlags::empty();
    let mut buffer = vec![];
    for (relative_offset, current_char) in text.char_indices() {
        // Word boundary.
        if !current_char.is_alphanumeric() {
            if !buffer.is_empty() {
                if flags.is_candidate_word() {
                    diagnostics.append(&mut buffer);
                }
                buffer.clear();
            }
            flags = WordFlags::empty();

            // Check if the boundary character is itself an ambiguous unicode character, in which
            // case, it's always included as a diagnostic.
            if !current_char.is_ascii() {
                if let Some(representant) = CONFUSABLES.get(&(current_char as u32)).copied() {
                    if let Some(diagnostic) = diagnostic_for_char(
                        current_char,
                        representant as char,
                        TextSize::try_from(relative_offset).unwrap() + range.start(),
                        context,
                        settings,
                    ) {
                        diagnostics.push(diagnostic);
                    }
                }
            }
        } else if current_char.is_ascii() {
            // The current word contains at least one ASCII character.
            flags |= WordFlags::ASCII;
        } else if let Some(representant) = CONFUSABLES.get(&(current_char as u32)).copied() {
            // The current word contains an ambiguous unicode character.
            if let Some(diagnostic) = diagnostic_for_char(
                current_char,
                representant as char,
                TextSize::try_from(relative_offset).unwrap() + range.start(),
                context,
                settings,
            ) {
                buffer.push(diagnostic);
            }
        } else {
            // The current word contains at least one unambiguous unicode character.
            flags |= WordFlags::UNAMBIGUOUS_UNICODE;
        }
    }

    // End of the text.
    if !buffer.is_empty() {
        if flags.is_candidate_word() {
            diagnostics.append(&mut buffer);
        }
        buffer.clear();
    }

    diagnostics
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
    const fn is_candidate_word(self) -> bool {
        self.contains(WordFlags::ASCII) || !self.contains(WordFlags::UNAMBIGUOUS_UNICODE)
    }
}

/// Create a [`Diagnostic`] to report an ambiguous unicode character.
fn diagnostic_for_char(
    confusable: char,
    representant: char,
    offset: TextSize,
    context: Context,
    settings: &Settings,
) -> Option<Diagnostic> {
    if !settings.allowed_confusables.contains(&confusable) {
        let char_range = TextRange::at(offset, confusable.text_len());
        let mut diagnostic = Diagnostic::new::<DiagnosticKind>(
            match context {
                Context::String => AmbiguousUnicodeCharacterString {
                    confusable,
                    representant,
                }
                .into(),
                Context::Docstring => AmbiguousUnicodeCharacterDocstring {
                    confusable,
                    representant,
                }
                .into(),
                Context::Comment => AmbiguousUnicodeCharacterComment {
                    confusable,
                    representant,
                }
                .into(),
            },
            char_range,
        );
        if settings.rules.enabled(diagnostic.kind.rule()) {
            if settings.rules.should_fix(diagnostic.kind.rule()) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    representant.to_string(),
                    char_range,
                )));
            }
            return Some(diagnostic);
        }
    }
    None
}
