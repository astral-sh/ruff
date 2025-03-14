use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_trivia::Cursor;
use ruff_text_size::{Ranged, TextRange};

use crate::noqa::{self, Directive, FileNoqaDirectives, NoqaDirectives};
use crate::Locator;

/// ## What it does
/// Check for `noqa` annotations that suppress all diagnostics, as opposed to
/// targeting specific diagnostics.
///
/// ## Why is this bad?
/// Suppressing all diagnostics can hide issues in the code.
///
/// Blanket `noqa` annotations are also more difficult to interpret and
/// maintain, as the annotation does not clarify which diagnostics are intended
/// to be suppressed.
///
/// ## Example
/// ```python
/// from .base import *  # noqa
/// ```
///
/// Use instead:
/// ```python
/// from .base import *  # noqa: F403
/// ```
///
/// ## Fix safety
/// This rule will attempt to fix blanket `noqa` annotations that appear to
/// be unintentional. For example, given `# noqa F401`, the rule will suggest
/// inserting a colon, as in `# noqa: F401`.
///
/// While modifying `noqa` comments is generally safe, doing so may introduce
/// additional diagnostics.
///
/// ## References
/// - [Ruff documentation](https://docs.astral.sh/ruff/configuration/#error-suppression)
#[derive(ViolationMetadata)]
pub(crate) struct BlanketNOQA {
    missing_colon: bool,
    file_exemption: bool,
}

impl Violation for BlanketNOQA {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlanketNOQA {
            missing_colon,
            file_exemption,
        } = self;
        // This awkward branching is necessary to ensure that the generic message is picked up by
        // `derive_message_formats`.
        if !missing_colon && !file_exemption {
            "Use specific rule codes when using `noqa`".to_string()
        } else if *file_exemption {
            "Use specific rule codes when using `ruff: noqa`".to_string()
        } else {
            "Use a colon when specifying `noqa` rule codes".to_string()
        }
    }

    fn fix_title(&self) -> Option<String> {
        if self.missing_colon {
            Some("Add missing colon".to_string())
        } else {
            None
        }
    }
}

/// PGH004
pub(crate) fn blanket_noqa(
    diagnostics: &mut Vec<Diagnostic>,
    noqa_directives: &NoqaDirectives,
    locator: &Locator,
    file_noqa_directives: &FileNoqaDirectives,
) {
    for line in file_noqa_directives.lines() {
        if let Directive::All(_) = line.parsed_file_exemption {
            diagnostics.push(Diagnostic::new(
                BlanketNOQA {
                    missing_colon: false,
                    file_exemption: true,
                },
                line.range(),
            ));
        }
    }

    for directive_line in noqa_directives.lines() {
        if let Directive::All(all) = &directive_line.directive {
            let line = locator.slice(directive_line);
            let noqa_end = all.end() - directive_line.start();

            // Skip the `# noqa`, plus any trailing whitespace.
            let mut cursor = Cursor::new(&line[noqa_end.to_usize()..]);
            cursor.eat_while(char::is_whitespace);

            if noqa::lex_codes(cursor.chars().as_str()).is_ok_and(|codes| !codes.is_empty()) {
                // Check for a missing colon.
                // Ex) `# noqa F401`
                let start = all.end();
                let end = start + cursor.token_len();
                let mut diagnostic = Diagnostic::new(
                    BlanketNOQA {
                        missing_colon: true,
                        file_exemption: false,
                    },
                    TextRange::new(all.start(), end),
                );
                diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(':'.to_string(), start)));
                diagnostics.push(diagnostic);
            } else {
                // Otherwise, it looks like an intentional blanket `noqa` annotation.
                diagnostics.push(Diagnostic::new(
                    BlanketNOQA {
                        missing_colon: false,
                        file_exemption: false,
                    },
                    all.range(),
                ));
            }
        }
    }
}
