use ruff_diagnostics::{Edit, Fix};
use ruff_python_trivia::indentation_at_offset;
use ruff_source_file::LineRanges;
use ruff_text_size::{TextLen, TextRange, TextSize};
use std::fmt::Write as _;

use crate::suppression::{
    CheckSuppressionsContext, Suppression, SuppressionKind, SuppressionTarget,
    UNUSED_IGNORE_COMMENT, UNUSED_TYPE_IGNORE_COMMENT,
};

/// Renders one unused directive, grouping adjacent unused codes from its comment.
pub(super) fn report_unused_suppression(
    context: &CheckSuppressionsContext,
    suppression: &Suppression,
    remaining: &[&Suppression],
    consumed: &mut usize,
    source: &str,
) {
    let mut unused_iter = remaining
        .iter()
        .enumerate()
        .filter(|(_, suppression)| !context.is_suppression_used(suppression.id()))
        .peekable();
    let unused_lint = match suppression.kind {
        SuppressionKind::Ty => &UNUSED_IGNORE_COMMENT,
        SuppressionKind::TypeIgnore => &UNUSED_TYPE_IGNORE_COMMENT,
    };

    let mut diag = match suppression.target {
        SuppressionTarget::All => {
            let Some(diag) = context.report_unchecked(unused_lint, suppression.range) else {
                return;
            };

            diag.into_diagnostic(format_args!(
                "Unused blanket `{}` directive",
                suppression.kind
            ))
        }
        SuppressionTarget::Lint(lint) => {
            // A single code in a `ty: ignore[<code1>, <code2>, ...]` directive

            // Is this the first code directly after the `[`?
            let includes_first_code = source[..suppression.range.start().to_usize()]
                .trim_end()
                .ends_with('[');

            let mut current = suppression;
            let mut unused_codes = Vec::new();

            // Group successive codes together into a single diagnostic,
            // or report the entire directive if all codes are unused.
            while let Some((index, next)) = unused_iter.peek() {
                if let SuppressionTarget::Lint(next_lint) = next.target
                    && next.comment_range == current.comment_range
                    && source[TextRange::new(current.range.end(), next.range.start())]
                        .chars()
                        .all(|c| c.is_whitespace() || c == ',')
                {
                    unused_codes.push(next_lint);
                    current = *next;
                    *consumed = *index + 1;
                    unused_iter.next();
                } else {
                    break;
                }
            }

            // Is the last suppression code the last code before the closing `]`.
            let includes_last_code = source[current.range.end().to_usize()..]
                .trim_start()
                .starts_with(']');

            // If only some codes are unused
            if !includes_first_code || !includes_last_code {
                let mut codes = format!("'{}'", lint.name());
                for code in &unused_codes {
                    let _ = write!(&mut codes, ", '{code}'", code = code.name());
                }

                if let Some(diag) = context.report_unchecked(
                    unused_lint,
                    TextRange::new(suppression.range.start(), current.range.end()),
                ) {
                    let mut diag = diag.into_diagnostic(format_args!(
                        "Unused `{kind}` directive: {codes}",
                        kind = suppression.kind
                    ));

                    diag.primary_annotation_mut()
                        .unwrap()
                        .push_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);

                    // Delete everything up to the start of the next code.
                    let trailing_len: TextSize = source[current.range.end().to_usize()..]
                        .chars()
                        .take_while(|c: &char| c.is_whitespace() || *c == ',')
                        .map(TextLen::text_len)
                        .sum();

                    // If we delete the last codes before `]`, ensure we delete any trailing comma
                    let leading_len: TextSize = if includes_last_code {
                        source[..suppression.range.start().to_usize()]
                            .chars()
                            .rev()
                            .take_while(|c: &char| c.is_whitespace() || *c == ',')
                            .map(TextLen::text_len)
                            .sum()
                    } else {
                        TextSize::default()
                    };

                    let fix_range = TextRange::new(
                        suppression.range.start() - leading_len,
                        current.range.end() + trailing_len,
                    );
                    diag.set_fix(Fix::safe_edit(Edit::range_deletion(fix_range)));

                    if unused_codes.is_empty() {
                        diag.help("Remove the unused suppression code");
                    } else {
                        diag.help("Remove the unused suppression codes");
                    }
                }

                return;
            }

            // All codes are unused
            let Some(diag) = context.report_unchecked(unused_lint, suppression.comment_range)
            else {
                return;
            };

            diag.into_diagnostic(format_args!(
                "Unused `{kind}` directive",
                kind = suppression.kind
            ))
        }
        SuppressionTarget::Empty => {
            let Some(diag) = context.report_unchecked(unused_lint, suppression.range) else {
                return;
            };
            diag.into_diagnostic(format_args!(
                "Unused `{kind}` without a code",
                kind = suppression.kind
            ))
        }
    };

    diag.primary_annotation_mut()
        .unwrap()
        .push_tag(ruff_db::diagnostic::DiagnosticTag::Unnecessary);
    diag.set_fix(remove_comment_fix(suppression, source));
    diag.help("Remove the unused suppression comment");
}

fn remove_comment_fix(suppression: &Suppression, source: &str) -> Fix {
    let comment_end = suppression.comment_range.end();
    let comment_start = suppression.comment_range.start();
    let after_comment = &source[comment_end.to_usize()..];

    if !after_comment.starts_with(['\n', '\r']) && !after_comment.is_empty() {
        // For example: `# ty: ignore # fmt: off`
        // Don't remove the trailing whitespace up to the `ty: ignore` comment
        let edit = Edit::range_deletion(suppression.comment_range);

        if indentation_at_offset(comment_start, source).is_some() {
            // Removing `# ty: ignore` from `# ty: ignore # fmt: off` would promote
            // `# fmt: off` to the primary own-line comment.
            return Fix::unsafe_edit(edit);
        }

        return Fix::safe_edit(edit);
    }

    if indentation_at_offset(comment_start, source).is_some() {
        return Fix::safe_edit(Edit::range_deletion(source.full_line_range(comment_start)));
    }

    // Remove any leading whitespace before the comment
    // to avoid unnecessary trailing whitespace once the comment is removed
    let before_comment = &source[..comment_start.to_usize()];

    let mut leading_len = TextSize::default();

    for c in before_comment.chars().rev() {
        match c {
            '\n' | '\r' => break,
            c if c.is_whitespace() => leading_len += c.text_len(),
            _ => break,
        }
    }

    Fix::safe_edit(Edit::range_deletion(TextRange::new(
        comment_start - leading_len,
        comment_end,
    )))
}
