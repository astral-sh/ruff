use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::token::TokenKind;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::Db;
use crate::lint::LintId;
use crate::suppression::{SuppressionTarget, suppressions};

/// Creates a fix for adding a suppression comment to suppress `lint` for `range`.
///
/// The fix prefers adding the code to an existing `ty: ignore[]` comment over
/// adding a new suppression comment.
pub fn create_suppression_fix(db: &dyn Db, file: File, id: LintId, range: TextRange) -> Fix {
    let suppressions = suppressions(db, file);
    let source = source_text(db, file);

    let mut existing_suppressions = suppressions.line_suppressions(range).filter(|suppression| {
        matches!(
            suppression.target,
            SuppressionTarget::Lint(_) | SuppressionTarget::Empty,
        )
    });

    // If there's an existing `ty: ignore[]` comment, append the code to it instead of creating a new suppression comment.
    if let Some(existing) = existing_suppressions.next() {
        let comment_text = &source[existing.comment_range];
        // Only add to the existing ignore comment if it has no reason.
        if let Some(before_closing_paren) = comment_text.trim_end().strip_suffix(']') {
            let up_to_last_code = before_closing_paren.trim_end();

            let insertion = if up_to_last_code.ends_with(',') {
                format!(" {id}", id = id.name())
            } else {
                format!(", {id}", id = id.name())
            };

            let relative_offset_from_end = comment_text.text_len() - up_to_last_code.text_len();

            return Fix::safe_edit(Edit::insertion(
                insertion,
                existing.comment_range.end() - relative_offset_from_end,
            ));
        }
    }

    // Always insert a new suppression at the end of the range to avoid having to deal with multiline strings
    // etc. Also make sure to not pass a sub-token range to `Tokens::after`.
    let parsed = parsed_module(db, file).load(db);
    let tokens = parsed.tokens().at_offset(range.end());
    let token_range = match tokens {
        ruff_python_ast::token::TokenAt::None => range,
        ruff_python_ast::token::TokenAt::Single(token) => token.range(),
        ruff_python_ast::token::TokenAt::Between(..) => range,
    };
    let tokens_after = parsed.tokens().after(token_range.end());

    // Same as for `line_end` when building up the `suppressions`: Ignore newlines
    // in multiline-strings, inside f-strings, or after a line continuation because we can't
    // place a comment on those lines.
    let line_end = tokens_after
        .iter()
        .find(|token| {
            matches!(
                token.kind(),
                TokenKind::Newline | TokenKind::NonLogicalNewline
            )
        })
        .map(Ranged::start)
        .unwrap_or(source.text_len());

    let up_to_line_end = &source[..line_end.to_usize()];
    let up_to_first_content = up_to_line_end.trim_end();
    let trailing_whitespace_len = up_to_line_end.text_len() - up_to_first_content.text_len();

    let insertion = format!("  # ty:ignore[{id}]", id = id.name());

    Fix::safe_edit(if trailing_whitespace_len == TextSize::ZERO {
        Edit::insertion(insertion, line_end)
    } else {
        // `expr # fmt: off<trailing_whitespace>`
        // Trim the trailing whitespace
        Edit::replacement(insertion, line_end - trailing_whitespace_len, line_end)
    })
}
