use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Formatter;

use ruff_db::diagnostic::LintName;
use ruff_db::display::FormatterJoinExtension;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::token::TokenKind;
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use smallvec::SmallVec;

use crate::Db;
use crate::lint::LintId;
use crate::suppression::{SuppressionKind, SuppressionTarget, Suppressions, suppressions};

/// Creates fixes to suppress all violations in `ids_with_range`.
///
/// This is different from calling `suppress_single` for every item in `ids_with_range`
/// in that errors on the same line are grouped together and ty will only insert a single
/// suppression with possibly multiple codes instead of adding multiple suppression comments.
pub fn suppress_all(
    db: &dyn Db,
    file: File,
    ids_with_range: &[(LintName, TextRange)],
) -> Vec<SuppressFix> {
    let suppressions = suppressions(db, file);
    let source = source_text(db, file);
    let parsed = parsed_module(db, file).load(db);
    let tokens = parsed.tokens();

    // Compute the full suppression ranges for each diagnostic.
    let mut ids_full_range: Vec<_> = ids_with_range
        .iter()
        .map(|&(id, range)| (id, suppression_range(db, file, range)))
        .collect();

    // Sort the suppression ranges by their start position and length (end position).
    // This ensures that a diagnostic with a shorter range is processed before
    // a diagnostic starting on the same line, but with a wider range (ends on a later line).
    //
    // ```
    // diag["home_assistant"]["entities"] = sorted(
    // ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ wider range
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~ narrower range
    //     diag["home_assistant"]["entities"], key=lambda ent: ent["entity_id"]
    // )  # end of the wider range
    // ^ wider range
    // ```
    //
    // This is important because a suppression inserted at the end of a narrower range
    // can result in a start-line suppression for a wider range. In the example above,
    // inserting a `ty:ignore` after `sorted(` suppresses the diagnostic with the narrower range
    // but also the diagnostic with the wider range (because the suppression is on its start line).
    ids_full_range.sort_unstable_by_key(|(_, range)| (range.start(), range.end()));

    let mut fixes = Vec::with_capacity(ids_full_range.len());

    // Tracks which lints get inserted by line. The offset is the line's start offset.
    // This is necessary to avoid inserting an end of line suppression if the diagnostic
    // was suppressed by inserting a suppression on its start line.
    // This also allows deduplicating suppressions for diagnostics with different ranges
    // where an end-suppression of one diagnostic becomes a start-suppression for another
    // (see the example with the wider range above).
    let mut by_line = BTreeMap::<TextSize, BTreeMap<LintName, SuppressionPosition>>::new();

    let mut by_suppression = BTreeMap::<TextSize, ExistingSuppressionGroup>::new();

    // 1. Try to add each diagnostic to an existing applicable `ty: ignore` comment, grouping
    //    diagnostics that resolve to the same comment into a single fix.
    for &(id, range) in &ids_full_range {
        let start_offset = range.start();
        if let Some(existing) = find_existing_suppression(suppressions, &source, start_offset) {
            by_line
                .entry(start_offset)
                .or_default()
                .insert(id, SuppressionPosition::StartLine);

            let group = by_suppression
                .entry(existing.insertion_offset)
                .or_insert_with(|| ExistingSuppressionGroup {
                    existing,
                    codes: BTreeSet::new(),
                    suppressed_diagnostics: 0,
                });
            group.codes.insert(id);
            group.suppressed_diagnostics += 1;
        }
    }

    for group in by_suppression.into_values() {
        let codes: SmallVec<[LintName; 2]> = group.codes.into_iter().collect();
        fixes.push(SuppressFix {
            fix: add_to_existing_suppression(group.existing, &codes),
            suppressed_diagnostics: group.suppressed_diagnostics,
        });
    }

    // 2. Group the diagnostics by their end position and try to add the code to an
    //    existing `ty: ignore` comment or insert a new `ty: ignore` comment.
    let mut by_end: BTreeMap<TextSize, (BTreeSet<LintName>, usize)> = BTreeMap::new();

    for (id, range) in ids_full_range {
        let suppression_position = by_line
            .get(&range.start())
            .and_then(|planned| planned.get(&id))
            .copied();

        match suppression_position {
            // Start-line suppressions already include all diagnostics that start on the same line.
            Some(SuppressionPosition::StartLine) => {}

            // If coverage comes from an other end-line suppression, count this diagnostic on that fix.
            Some(SuppressionPosition::EndLine(end_offset)) => {
                let (_, suppressed_diagnostics) = by_end.entry(end_offset).or_default();
                *suppressed_diagnostics += 1;
            }

            None => {
                let (lints, suppressed_diagnostics) = by_end.entry(range.end()).or_default();
                lints.insert(id);
                *suppressed_diagnostics += 1;

                // Record the physical line where this end-line suppression will be inserted so wider
                // same-code ranges starting there can be recognized as already covered.
                by_line
                    .entry(line_start(tokens, range.end()))
                    .or_default()
                    .entry(id)
                    .or_insert(SuppressionPosition::EndLine(range.end()));
            }
        }
    }

    for (end_offset, (lints, suppressed_diagnostics)) in by_end {
        let codes: SmallVec<[LintName; 2]> = lints.into_iter().collect();

        fixes.push(SuppressFix {
            fix: append_to_existing_or_add_end_of_line_suppression(
                suppressions,
                &source,
                &codes,
                end_offset,
            ),
            suppressed_diagnostics,
        });
    }

    fixes
}

#[derive(Copy, Clone)]
enum SuppressionPosition {
    StartLine,
    EndLine(TextSize),
}

/// Diagnostics that can be suppressed by a single edit to an existing suppression.
///
/// `codes` is deduplicated for insertion, while `suppressed_diagnostics` counts every diagnostic,
/// including multiple diagnostics with the same lint code.
struct ExistingSuppressionGroup {
    existing: ExistingSuppression,
    codes: BTreeSet<LintName>,
    suppressed_diagnostics: usize,
}

/// Fix to suppress one or more diagnostics.
pub struct SuppressFix {
    pub fix: Fix,
    /// The number of diagnostics that will be suppressed if this fix is applied.
    pub suppressed_diagnostics: usize,
}

/// Creates a fix to suppress a single lint.
pub fn suppress_single(db: &dyn Db, file: File, id: LintId, range: TextRange) -> Fix {
    let suppression_range = suppression_range(db, file, range);

    let suppressions = suppressions(db, file);
    let source = source_text(db, file);
    let codes = &[id.name()];

    if let Some(existing) =
        find_existing_suppression(suppressions, &source, suppression_range.start())
    {
        return add_to_existing_suppression(existing, codes);
    }

    append_to_existing_or_add_end_of_line_suppression(
        suppressions,
        &source,
        codes,
        suppression_range.end(),
    )
}

/// Returns the suppression range for the given `range`.
///
/// The suppression range is defined as:
///
/// * `start`: The `end` of the preceding `Newline` or `NonLogicalLine` token.
/// * `end`: The `start` of the first `NonLogicalLine` or `Newline` token coming after the range.
///
/// For most ranges, this means the suppression range starts at the beginning of the physical line
/// and ends at the end of the physical line containing `range`. The exceptions to this are:
///
/// * If `range` is within a single-line interpolated expression, then the start and end are extended to the start and end of the enclosing interpolated string.
/// * If there's a line continuation, then the suppression range is extended to include the following line too.
/// * If there's a multiline string, then the suppression range is extended to cover the starting and ending line of the multiline string.
fn suppression_range(db: &dyn Db, file: File, range: TextRange) -> TextRange {
    // Always insert a new suppression at the end of the range to avoid having to deal with multiline strings
    // etc. Also make sure to not pass a sub-token range to `Tokens::after`.
    let parsed = parsed_module(db, file).load(db);
    let line_start = line_start(parsed.tokens(), range.start());

    let after_token_range = match parsed.tokens().at_offset(range.end()) {
        ruff_python_ast::token::TokenAt::None => range,
        ruff_python_ast::token::TokenAt::Single(token) => token.range(),
        ruff_python_ast::token::TokenAt::Between(..) => range,
    };
    let after_tokens = parsed.tokens().after(after_token_range.end());
    let line_end = after_tokens
        .iter()
        .find(|token| {
            matches!(
                token.kind(),
                TokenKind::Newline | TokenKind::NonLogicalNewline
            )
        })
        .map(Ranged::start)
        .unwrap_or(range.end());

    TextRange::new(line_start, line_end)
}

fn line_start(tokens: &ruff_python_ast::token::Tokens, offset: TextSize) -> TextSize {
    tokens
        .before(tokens.token_range(offset).start())
        .iter()
        .rfind(|token| {
            matches!(
                token.kind(),
                TokenKind::Newline | TokenKind::NonLogicalNewline
            )
        })
        .map(Ranged::end)
        .unwrap_or_default()
}

fn append_to_existing_or_add_end_of_line_suppression(
    suppressions: &Suppressions,
    source: &str,
    codes: &[LintName],
    line_end: TextSize,
) -> Fix {
    if let Some(existing) = find_existing_suppression(suppressions, source, line_end) {
        return add_to_existing_suppression(existing, codes);
    }

    let up_to_line_end = &source[..line_end.to_usize()];
    // Don't use `trim_end` in case the previous line ends with a `\` followed by a newline. We don't want to eat
    // into that newline!
    let up_to_first_content =
        up_to_line_end.trim_end_matches(|c| !matches!(c, '\n' | '\r') && c.is_whitespace());
    let trailing_whitespace_len = up_to_line_end.text_len() - up_to_first_content.text_len();

    let insertion = format!(
        "  # ty:ignore[{codes}]",
        codes = Codes(SuppressionKind::Ty, codes)
    );

    Fix::safe_edit(if trailing_whitespace_len == TextSize::ZERO {
        Edit::insertion(insertion, line_end)
    } else {
        // `expr # fmt: off<trailing_whitespace>`
        // Trim the trailing whitespace
        Edit::replacement(insertion, line_end - trailing_whitespace_len, line_end)
    })
}

fn find_existing_suppression(
    suppressions: &Suppressions,
    source: &str,
    offset: TextSize,
) -> Option<ExistingSuppression> {
    let line_start = source.line_start(offset);
    let existing = suppressions
        .inline_suppressions(TextRange::empty(offset))
        .find(|suppression| {
            source.line_start(suppression.comment_range.start()) == line_start
                && matches!(
                    suppression.target,
                    SuppressionTarget::Lint(_) | SuppressionTarget::Empty,
                )
        })?;
    let comment_text = &source[existing.comment_range];

    // Only add to the existing ignore comment if it has no reason.
    let before_closing_bracket = comment_text.trim_end().strip_suffix(']')?;
    let up_to_last_code = before_closing_bracket.trim_end();
    let separator = if up_to_last_code.ends_with('[') {
        ""
    } else if up_to_last_code.ends_with(',') {
        " "
    } else {
        ", "
    };
    let relative_offset_from_end = comment_text.text_len() - up_to_last_code.text_len();

    Some(ExistingSuppression {
        insertion_offset: existing.comment_range.end() - relative_offset_from_end,
        kind: existing.kind,
        separator,
    })
}

/// Appends `codes` to an existing suppression comment.
fn add_to_existing_suppression(existing: ExistingSuppression, codes: &[LintName]) -> Fix {
    let separator = existing.separator;
    let insertion = format!("{separator}{codes}", codes = Codes(existing.kind, codes));

    Fix::safe_edit(Edit::insertion(insertion, existing.insertion_offset))
}

/// The location and formatting required to append lint codes to an existing suppression comment.
///
/// `kind` determines how codes are rendered, while `separator` preserves the syntax around the
/// existing final code. For example:
///
/// ```python
/// # ty: ignore[division-by-zero]
/// ```
///
/// has an insertion offset before `]` and uses `", "` as the separator.
#[derive(Copy, Clone)]
struct ExistingSuppression {
    insertion_offset: TextSize,
    kind: SuppressionKind,
    separator: &'static str,
}

struct Codes<'a>(SuppressionKind, &'a [LintName]);

impl std::fmt::Display for Codes<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut joiner = f.join(", ");

        let namespace = if self.0.is_type_ignore() { "ty:" } else { "" };

        for item in self.1 {
            joiner.entry(&format_args!("{namespace}{item}"));
        }

        joiner.finish()
    }
}
