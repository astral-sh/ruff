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
use smallvec::SmallVec;

use crate::Db;
use crate::lint::LintId;
use crate::suppression::{SuppressionKind, SuppressionTarget, Suppressions, suppressions};

/// Creates fixes to suppress all violations in `ids_with_range`.
///
/// This is different from calling `suppress_single` for every item in `ids_with_range`
/// in that errors on the same line are grouped together and ty will only insert a single
/// suppression with possibly multiple codes instead of adding multiple suppression comments.
pub fn suppress_all(db: &dyn Db, file: File, ids_with_range: &[(LintName, TextRange)]) -> Vec<Fix> {
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

    // 1. Group the diagnostics by their line-start position and try to add
    //    the suppression to an existing `ty: ignore` comment on that line.
    let mut by_start: BTreeMap<_, BTreeSet<LintName>> = BTreeMap::new();

    for &(id, range) in &ids_full_range {
        let lints = by_start.entry(range.start()).or_default();
        lints.insert(id);
    }

    let mut fixes = Vec::with_capacity(ids_full_range.len());

    // Tracks which lints get inserted by line. The offset is the line's start offset.
    // This is necessary to avoid inserting an end of line suppression if the diagnostic
    // was suppressed by inserting a suppression on its start line.
    // This also allows deduplicating suppressions for diagnostics with different ranges
    // where an end-suppression of one diagnostic becomes a start-suppression for another
    // (see the example with the wider range above).
    let mut by_line = BTreeMap::<TextSize, BTreeSet<LintName>>::new();

    for (start_offset, lints) in by_start {
        let codes: SmallVec<[LintName; 2]> = lints.into_iter().collect();
        if let Some(add_to_start) =
            add_to_existing_suppression(suppressions, &source, &codes, start_offset)
        {
            by_line
                .entry(start_offset)
                .or_default()
                .extend(codes.iter().copied());
            fixes.push(add_to_start);
        }
    }

    // 2. Group the diagnostics by their end position and try to add the code to an
    //    existing `ty: ignore` comment or insert a new `ty: ignore` comment.
    let mut by_end: BTreeMap<TextSize, BTreeSet<LintName>> = BTreeMap::new();

    for (id, range) in ids_full_range {
        // Skip end-line suppressions when we already inserted a same-code suppression on the
        // range's start line. This happens either because we appended to an existing ignore
        // comment on that line, or because a narrower multiline range ends on that same line.
        if by_line
            .get(&range.start())
            .is_some_and(|planned_codes| planned_codes.contains(&id))
        {
            continue;
        }

        by_end.entry(range.end()).or_default().insert(id);
        // Record the physical line where this end-line suppression will be inserted so wider
        // same-code ranges starting there can be recognized as already covered.
        by_line
            .entry(line_start(tokens, range.end()))
            .or_default()
            .insert(id);
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
    let token_range = match tokens.at_offset(offset) {
        ruff_python_ast::token::TokenAt::None => TextRange::empty(offset),
        ruff_python_ast::token::TokenAt::Single(token) => token.range(),
        ruff_python_ast::token::TokenAt::Between(..) => TextRange::empty(offset),
    };

    tokens
        .before(token_range.start())
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
    if let Some(add_fix) = add_to_existing_suppression(suppressions, source, codes, line_end) {
        return add_fix;
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
        format!(" {codes}", codes = Codes(existing.kind, codes))
    } else {
        format!(", {codes}", codes = Codes(existing.kind, codes))
    };

    let relative_offset_from_end = comment_text.text_len() - up_to_last_code.text_len();

    Some(Fix::safe_edit(Edit::insertion(
        insertion,
        existing.comment_range.end() - relative_offset_from_end,
    )))
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
