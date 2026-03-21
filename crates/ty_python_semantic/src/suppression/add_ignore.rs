use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Formatter;

use ruff_db::diagnostic::LintName;
use ruff_db::display::FormatterJoinExtension;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::token::TokenKind;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::Db;
use crate::lint::LintId;
use crate::suppression::{SuppressionTarget, Suppressions, suppressions};

/// Creates fixes to suppress all violations in `ids_with_range`.
///
/// This is different from calling `suppress_single` for every item in `ids_with_range`
/// in that errors on the same line are grouped together and ty will only insert a single
/// suppression with possibly multiple codes instead of adding multiple suppression comments.
pub fn suppress_all(db: &dyn Db, file: File, ids_with_range: &[(LintName, TextRange)]) -> Vec<Fix> {
    let suppressions = suppressions(db, file);
    let source = source_text(db, file);

    // Compute the full suppression ranges for each diagnostic.
    let ids_full_range: Vec<_> = ids_with_range
        .iter()
        .map(|&(id, range)| (id, suppression_range(db, file, range)))
        .collect();

    // 1. Group the diagnostics by their line-start position and try to add
    //    the suppression to an existing `ty: ignore` comment on that line.
    let mut by_start: BTreeMap<_, (BTreeSet<LintName>, SmallVec<[usize; 2]>)> = BTreeMap::new();

    for (i, &(id, range)) in ids_full_range.iter().enumerate() {
        let (lints, indices) = by_start.entry(range.start()).or_default();
        lints.insert(id);
        indices.push(i);
    }

    let mut fixes = Vec::with_capacity(ids_full_range.len());

    // Tracks the indices in `ids_with_range` for which we pushed a
    // fix to `fixes`
    let mut fixed = FxHashSet::default();

    for (start_offset, (lints, original_indices)) in by_start {
        let codes: SmallVec<[LintName; 2]> = lints.into_iter().collect();
        if let Some(add_to_start) =
            add_to_existing_suppression(suppressions, &source, &codes, start_offset)
        {
            // Mark the diagnostics as fixed, so that we don't generate a fix at the end of the line.
            fixed.extend(original_indices);
            fixes.push(add_to_start);
        }
    }

    // 2. Group the diagnostics by their end position and try to add the code to an
    //    existing `ty: ignore` comment or insert a new `ty: ignore` comment. But only do this
    //    for diagnostics for which we haven't pushed a start-line fix.
    let mut by_end: BTreeMap<TextSize, BTreeSet<LintName>> = BTreeMap::new();

    for (i, (id, range)) in ids_full_range.into_iter().enumerate() {
        if fixed.contains(&i) {
            // We already pushed a fix that appends the suppression to an existing suppression on the
            // start line.
            continue;
        }

        by_end.entry(range.end()).or_default().insert(id);
    }

    for (end_offset, lints) in by_end {
        let codes: SmallVec<[LintName; 2]> = lints.into_iter().collect();

        fixes.push(append_to_existing_or_add_end_of_line_suppression(
            suppressions,
            &source,
            &codes,
            end_offset,
        ));
    }

    fixes.sort_by_key(ruff_diagnostics::Fix::min_start);

    fixes
}

/// Creates a fix to suppress a single lint.
pub fn suppress_single(db: &dyn Db, file: File, id: LintId, range: TextRange) -> Fix {
    let suppression_range = suppression_range(db, file, range);

    let suppressions = suppressions(db, file);
    let source = source_text(db, file);
    let codes = &[id.name()];

    if let Some(add_fix) =
        add_to_existing_suppression(suppressions, &source, codes, suppression_range.start())
    {
        return add_fix;
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
    let before_token_range = match parsed.tokens().at_offset(range.start()) {
        ruff_python_ast::token::TokenAt::None => range,
        ruff_python_ast::token::TokenAt::Single(token) => token.range(),
        ruff_python_ast::token::TokenAt::Between(..) => range,
    };
    let before_tokens = parsed.tokens().before(before_token_range.start());

    let line_start = before_tokens
        .iter()
        .rfind(|token| {
            matches!(
                token.kind(),
                TokenKind::Newline | TokenKind::NonLogicalNewline
            )
        })
        .map(Ranged::end)
        .unwrap_or(TextSize::default());

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

fn append_to_existing_or_add_end_of_line_suppression(
    suppressions: &Suppressions,
    source: &str,
    codes: &[LintName],
    line_end: TextSize,
) -> Fix {
    if let Some(add_fix) = add_to_existing_suppression(suppressions, source, codes, line_end) {
        return add_fix;
    }

    let up_to_line_end = &source[..line_end.to_usize()];
    // Don't use `trim_end` in case the previous line ends with a `\` followed by a newline. We don't want to eat
    // into that newline!
    let up_to_first_content =
        up_to_line_end.trim_end_matches(|c| !matches!(c, '\n' | '\r') && c.is_whitespace());
    let trailing_whitespace_len = up_to_line_end.text_len() - up_to_first_content.text_len();

    let insertion = format!("  # ty:ignore[{codes}]", codes = Codes(codes));

    Fix::safe_edit(if trailing_whitespace_len == TextSize::ZERO {
        Edit::insertion(insertion, line_end)
    } else {
        // `expr # fmt: off<trailing_whitespace>`
        // Trim the trailing whitespace
        Edit::replacement(insertion, line_end - trailing_whitespace_len, line_end)
    })
}

fn add_to_existing_suppression(
    suppressions: &Suppressions,
    source: &str,
    codes: &[LintName],
    offset: TextSize,
) -> Option<Fix> {
    let mut existing_suppressions = suppressions
        .line_suppressions(TextRange::empty(offset))
        .filter(|suppression| {
            matches!(
                suppression.target,
                SuppressionTarget::Lint(_) | SuppressionTarget::Empty,
            )
        });

    // If there's an existing `ty: ignore[]` comment, append the code to it instead of creating a new suppression comment.
    let existing = existing_suppressions.next()?;
    let comment_text = &source[existing.comment_range];

    // Only add to the existing ignore comment if it has no reason.
    let before_closing_paren = comment_text.trim_end().strip_suffix(']')?;
    let up_to_last_code = before_closing_paren.trim_end();

    let insertion = if up_to_last_code.ends_with(',') {
        format!(" {codes}", codes = Codes(codes))
    } else {
        format!(", {codes}", codes = Codes(codes))
    };

    let relative_offset_from_end = comment_text.text_len() - up_to_last_code.text_len();

    Some(Fix::safe_edit(Edit::insertion(
        insertion,
        existing.comment_range.end() - relative_offset_from_end,
    )))
}

struct Codes<'a>(&'a [LintName]);

impl std::fmt::Display for Codes<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.join(", ").entries(self.0).finish()
    }
}
