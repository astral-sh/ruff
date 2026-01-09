use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_text_size::{TextLen, TextRange, TextSize};
use std::fmt::Write as _;

use crate::lint::LintId;
use crate::suppression::{
    CheckSuppressionsContext, Suppression, SuppressionTarget, UNUSED_IGNORE_COMMENT,
};

/// Checks for unused suppression comments in `file` and
/// adds diagnostic for each of them to `diagnostics`.
///
/// Does nothing if the [`UNUSED_IGNORE_COMMENT`] rule is disabled.
pub(super) fn check_unused_suppressions(context: &mut CheckSuppressionsContext) {
    if context.is_lint_disabled(&UNUSED_IGNORE_COMMENT) {
        return;
    }

    let diagnostics = context.diagnostics.get_mut();

    let all = context.suppressions;
    let mut unused = Vec::with_capacity(
        all.file
            .len()
            .saturating_add(all.line.len())
            .saturating_sub(diagnostics.used_len()),
    );

    // Collect all suppressions that are unused after type-checking.
    for suppression in all {
        if diagnostics.is_used(suppression.id()) {
            continue;
        }

        // `unused-ignore-comment` diagnostics can only be suppressed by specifying a
        // code. This is necessary because every `type: ignore` would implicitly also
        // suppress its own unused-ignore-comment diagnostic.
        if let Some(unused_suppression) = all
            .lint_suppressions(suppression.range, LintId::of(&UNUSED_IGNORE_COMMENT))
            .find(|unused_ignore_suppression| unused_ignore_suppression.target.is_lint())
        {
            // A `unused-ignore-comment` suppression can't ignore itself.
            // It can only ignore other suppressions.
            if unused_suppression.id() != suppression.id() {
                diagnostics.mark_used(unused_suppression.id());
                continue;
            }
        }

        unused.push(suppression);
    }

    let mut unused_iter = unused
        .iter()
        .filter(|suppression| {
            // This looks silly but it's necessary to check again if a `unused-ignore-comment` is indeed unused
            // in case the "unused" directive comes after it:
            // ```py
            // a = 10 / 2  # ty: ignore[unused-ignore-comment, division-by-zero]
            // ```
            !context.is_suppression_used(suppression.id())
        })
        .peekable();

    let source = source_text(context.db, context.file);

    while let Some(suppression) = unused_iter.next() {
        let mut diag = match suppression.target {
            SuppressionTarget::All => {
                let Some(diag) =
                    context.report_unchecked(&UNUSED_IGNORE_COMMENT, suppression.range)
                else {
                    continue;
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
                while let Some(next) = unused_iter.peek() {
                    if let SuppressionTarget::Lint(next_lint) = next.target
                        && next.comment_range == current.comment_range
                        && source[TextRange::new(current.range.end(), next.range.start())]
                            .chars()
                            .all(|c| c.is_whitespace() || c == ',')
                    {
                        unused_codes.push(next_lint);
                        current = *next;
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
                        &UNUSED_IGNORE_COMMENT,
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

                    continue;
                }

                // All codes are unused
                let Some(diag) =
                    context.report_unchecked(&UNUSED_IGNORE_COMMENT, suppression.comment_range)
                else {
                    continue;
                };

                diag.into_diagnostic(format_args!(
                    "Unused `{kind}` directive",
                    kind = suppression.kind
                ))
            }
            SuppressionTarget::Empty => {
                let Some(diag) =
                    context.report_unchecked(&UNUSED_IGNORE_COMMENT, suppression.range)
                else {
                    continue;
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
        diag.set_fix(remove_comment_fix(suppression, &source));
        diag.help("Remove the unused suppression comment");
    }
}

fn remove_comment_fix(suppression: &Suppression, source: &str) -> Fix {
    let comment_end = suppression.comment_range.end();
    let comment_start = suppression.comment_range.start();
    let after_comment = &source[comment_end.to_usize()..];

    if !after_comment.starts_with(['\n', '\r']) {
        // For example: `# ty: ignore # fmt: off`
        // Don't remove the trailing whitespace up to the `ty: ignore` comment
        return Fix::safe_edit(Edit::range_deletion(suppression.comment_range));
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
