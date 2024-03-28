use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::noqa::Directive;

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
/// ## References
/// - [Ruff documentation](https://docs.astral.sh/ruff/configuration/#error-suppression)
#[violation]
pub struct BlanketNOQA {
    missing_colon: bool,
    space_before_colon: bool,
}

impl Violation for BlanketNOQA {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlanketNOQA {
            missing_colon,
            space_before_colon,
        } = self;
        if *missing_colon {
            return format!("Use a colon when specifying `noqa` rule codes");
        }
        if *space_before_colon {
            return format!("Do not add spaces between `noqa` and its colon");
        }
        format!("Use specific rule codes when using `noqa`")
    }

    fn fix_title(&self) -> Option<String> {
        let BlanketNOQA {
            missing_colon,
            space_before_colon,
        } = self;
        if *missing_colon {
            return Some("Add missing colon".to_string());
        }
        if *space_before_colon {
            return Some("Remove space(s) before colon".to_string());
        }
        None
    }
}

/// PGH004
pub(crate) fn blanket_noqa(
    diagnostics: &mut Vec<Diagnostic>,
    indexer: &Indexer,
    locator: &Locator,
) {
    for range in indexer.comment_ranges() {
        let line = locator.slice(*range);
        let offset = range.start();
        if let Ok(Some(Directive::All(all))) = Directive::try_extract(line, TextSize::new(0)) {
            let noqa_start = offset + all.range().start();
            let noqa_end = offset + all.range().end();
            let post_noqa_start = all.range().end().to_usize();
            let mut cursor = post_noqa_start;
            cursor += leading_whitespace_len(&line[cursor..]);

            // Check for extraneous space before colon
            if matches!(line[cursor..].chars().next(), Some(':')) {
                let start = offset + all.range().end();
                let end = offset + TextSize::new(u32::try_from(cursor).unwrap());
                let mut diagnostic = Diagnostic::new(
                    BlanketNOQA {
                        missing_colon: false,
                        space_before_colon: true,
                    },
                    TextRange::new(noqa_start, end),
                );
                diagnostic.set_fix(Fix::unsafe_edit(Edit::deletion(start, end)));
                diagnostics.push(diagnostic);
            }
            // Check for missing colon
            else if Directive::lex_code(&line[cursor..]).is_some() {
                let start = offset + all.range().end();
                let end = start + TextSize::new(1);
                let mut diagnostic = Diagnostic::new(
                    BlanketNOQA {
                        missing_colon: true,
                        space_before_colon: false,
                    },
                    TextRange::new(noqa_start, end),
                );
                diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(':'.to_string(), start)));
                diagnostics.push(diagnostic);
            } else {
                diagnostics.push(Diagnostic::new(
                    BlanketNOQA {
                        missing_colon: false,
                        space_before_colon: false,
                    },
                    TextRange::new(noqa_start, noqa_end),
                ));
            }
        }
    }
}

fn leading_whitespace_len(text: &str) -> usize {
    text.find(|c: char| !c.is_whitespace()).unwrap_or(0)
}
