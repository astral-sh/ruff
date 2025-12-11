use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::suppression::{InvalidSuppressionKind, ParseErrorKind};
use crate::{AlwaysFixableViolation, Violation};

/// ## What it does
/// Checks for invalid suppression comments
///
/// ## Why is this bad?
/// Invalid suppression comments are ignored by Ruff, and should either
/// be fixed or removed to avoid confusion.
///
/// ## Example
/// ```python
/// ruff: disable  # missing codes
/// ```
///
/// Use instead:
/// ```python
/// # ruff: disable[E501]
/// ```
///
/// Or delete the invalid suppression comment.
///
/// ## References
/// - [Ruff error suppression](https://docs.astral.sh/ruff/linter/#error-suppression)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.9")]
pub(crate) struct InvalidSuppressionComment {
    pub kind: InvalidSuppressionCommentKind,
}

impl Violation for InvalidSuppressionComment {
    #[derive_message_formats]
    fn message(&self) -> String {
        let msg = match self.kind {
            InvalidSuppressionCommentKind::Invalid(InvalidSuppressionKind::Indentation) => {
                "unexpected indentation".to_string()
            }
            InvalidSuppressionCommentKind::Invalid(InvalidSuppressionKind::Trailing) => {
                "trailing comments are not supported".to_string()
            }
            InvalidSuppressionCommentKind::Invalid(InvalidSuppressionKind::Unmatched) => {
                "no matching 'disable' comment".to_string()
            }
            InvalidSuppressionCommentKind::Error(error) => format!("{error}"),
        };
        format!("Invalid suppression comment: {msg}")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Remove invalid suppression comment".to_string())
    }
}

pub(crate) enum InvalidSuppressionCommentKind {
    Invalid(InvalidSuppressionKind),
    Error(ParseErrorKind),
}
