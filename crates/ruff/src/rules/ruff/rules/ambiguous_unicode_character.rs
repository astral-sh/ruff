use ruff_text_size::{TextLen, TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, DiagnosticKind, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;

use crate::registry::AsRule;
use crate::rules::ruff::rules::confusables::CONFUSABLES;
use crate::rules::ruff::rules::Context;
use crate::settings::{flags, Settings};

#[violation]
pub struct AmbiguousUnicodeCharacterString {
    pub confusable: char,
    pub representant: char,
}

impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterString {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterString {
            confusable,
            representant,
        } = self;
        format!(
            "String contains ambiguous unicode character `{confusable}` (did you mean \
             `{representant}`?)"
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
    pub confusable: char,
    pub representant: char,
}

impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterDocstring {
            confusable,
            representant,
        } = self;
        format!(
            "Docstring contains ambiguous unicode character `{confusable}` (did you mean \
             `{representant}`?)"
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
    pub confusable: char,
    pub representant: char,
}

impl AlwaysAutofixableViolation for AmbiguousUnicodeCharacterComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let AmbiguousUnicodeCharacterComment {
            confusable,
            representant,
        } = self;
        format!(
            "Comment contains ambiguous unicode character `{confusable}` (did you mean \
             `{representant}`?)"
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

pub fn ambiguous_unicode_character(
    locator: &Locator,
    range: TextRange,
    context: Context,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

    let text = locator.slice(range);

    for (relative_offset, current_char) in text.char_indices() {
        if !current_char.is_ascii() {
            // Search for confusing characters.
            if let Some(representant) = CONFUSABLES.get(&(current_char as u32)).copied() {
                if !settings.allowed_confusables.contains(&current_char) {
                    let char_range = TextRange::at(
                        TextSize::try_from(relative_offset).unwrap() + range.start(),
                        current_char.text_len(),
                    );

                    let mut diagnostic = Diagnostic::new::<DiagnosticKind>(
                        match context {
                            Context::String => AmbiguousUnicodeCharacterString {
                                confusable: current_char,
                                representant: representant as char,
                            }
                            .into(),
                            Context::Docstring => AmbiguousUnicodeCharacterDocstring {
                                confusable: current_char,
                                representant: representant as char,
                            }
                            .into(),
                            Context::Comment => AmbiguousUnicodeCharacterComment {
                                confusable: current_char,
                                representant: representant as char,
                            }
                            .into(),
                        },
                        char_range,
                    );
                    if settings.rules.enabled(diagnostic.kind.rule()) {
                        if autofix.into() && settings.rules.should_fix(diagnostic.kind.rule()) {
                            diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                                (representant as char).to_string(),
                                char_range,
                            )));
                        }
                        diagnostics.push(diagnostic);
                    }
                }
            }
        }
    }

    diagnostics
}
