//! Helpers for adding suppression comments without changing which existing suppression is used.
//!
//! An applicable same-line or nested own-line suppression is extended first because it has the
//! narrowest scope. For diagnostics spanning multiple lines, an opening-line suppression takes
//! precedence over a separate closing-line suppression, matching normal suppression resolution.
//! Comments with trailing reasons are never extended: preserving the reason requires adding a
//! separate suppression instead.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Formatter;

use ruff_db::diagnostic::LintName;
use ruff_db::display::FormatterJoinExtension;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::token::TokenKind;
use ruff_python_trivia::{indentation_at_offset, leading_indentation};
use ruff_source_file::{LineRanges, find_newline};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};
use smallvec::SmallVec;

use crate::Db;
use crate::lint::LintId;
use crate::suppression::{
    SuppressionKind, Suppressions, is_suppression_comment_lint, select_preferred_suppression,
    suppressions,
};

/// Creates fixes to suppress all violations in `ids_with_range`.
///
/// Unlike calling [`suppress_single`] for each diagnostic, this groups diagnostics that can share
/// an edit. It appends codes once to each applicable existing suppression and otherwise inserts at
/// most one end-of-line suppression at each destination. Every returned [`SuppressFix`] records
/// how many diagnostics its edit accounts for.
pub fn suppress_all(
    db: &dyn Db,
    file: File,
    ids_with_range: &[(LintName, TextRange)],
) -> Vec<SuppressFix> {
    let suppressions = suppressions(db, file);
    let source = source_text(db, file);
    let parsed = parsed_module(db, file).load(db);
    let tokens = parsed.tokens();

    let mut line_local = BTreeMap::<TextSize, (TextSize, BTreeSet<LintName>, usize)>::new();
    let mut ids_with_suppression_range = Vec::with_capacity(ids_with_range.len());

    for &(id, diagnostic_range) in ids_with_range {
        if is_suppression_comment_lint(id) {
            match suppression_comment_fix(db, file, diagnostic_range) {
                Some(SuppressionCommentFix::LineLocal(start))
                    if find_existing_suppression(suppressions, &source, diagnostic_range, id)
                        .is_none() =>
                {
                    let (insertion_start, lints, suppressed_diagnostics) = line_local
                        .entry(source.line_start(start))
                        .or_insert_with(|| (start, BTreeSet::new(), 0));
                    *insertion_start = (*insertion_start).min(start);
                    lints.insert(id);
                    *suppressed_diagnostics += 1;
                    continue;
                }
                Some(
                    SuppressionCommentFix::LineLocal(_)
                    | SuppressionCommentFix::SameLine
                    | SuppressionCommentFix::Shebang,
                ) => {}
                None => continue,
            }
        }

        ids_with_suppression_range.push((
            id,
            diagnostic_range,
            suppression_range(db, file, diagnostic_range),
        ));
    }

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
    ids_with_suppression_range.sort_unstable_by_key(|(_, _, range)| (range.start(), range.end()));

    let mut fixes = Vec::with_capacity(ids_with_suppression_range.len() + line_local.len());
    let mut with_existing = Vec::new();
    let mut without_existing = Vec::new();

    fixes.extend(
        line_local
            .into_values()
            .map(|(start, lints, suppressed_diagnostics)| SuppressFix {
                fix: add_line_local_suppression(
                    &source,
                    &lints.into_iter().collect::<SmallVec<[_; 2]>>(),
                    start,
                ),
                suppressed_diagnostics,
            }),
    );

    // Choose the final existing suppression for every diagnostic before grouping any edits.
    for (id, diagnostic_range, suppression_range) in ids_with_suppression_range {
        if let Some(existing) =
            find_existing_suppression(suppressions, &source, diagnostic_range, id)
        {
            with_existing.push((id, suppression_range, existing));
        } else {
            without_existing.push((id, suppression_range));
        }
    }

    // Tracks newly inserted end-of-line suppressions by the physical line where they become start
    // suppressions. This avoids inserting another suppression for a wider same-code diagnostic
    // that starts on that line (see the example above).
    let mut by_line = BTreeMap::<TextSize, BTreeMap<LintName, TextSize>>::new();
    let mut by_end: BTreeMap<TextSize, (BTreeSet<LintName>, usize)> = BTreeMap::new();

    for (id, range) in without_existing {
        let existing_end = by_line
            .get(&range.start())
            .and_then(|planned| planned.get(&id))
            .copied();

        if let Some(end_offset) = existing_end {
            let (_, suppressed_diagnostics) = by_end.entry(end_offset).or_default();
            *suppressed_diagnostics += 1;
            continue;
        }

        let (lints, suppressed_diagnostics) = by_end.entry(range.end()).or_default();
        lints.insert(id);
        *suppressed_diagnostics += 1;

        by_line
            .entry(line_start(tokens, range.end()))
            .or_default()
            .entry(id)
            .or_insert(range.end());
    }

    let mut by_suppression =
        BTreeMap::<TextSize, (ExistingSuppression, BTreeSet<LintName>, usize)>::new();

    // Reconcile existing-comment edits after planning new suppressions. A new suppression inserted
    // at the end of a narrower range can cover the start of a wider diagnostic and make an edit to
    // the wider diagnostic's existing end-line suppression immediately unused.
    for (id, range, existing) in with_existing {
        if let Some(end_offset) = by_line
            .get(&range.start())
            .and_then(|planned| planned.get(&id))
        {
            let (_, suppressed_diagnostics) = by_end.entry(*end_offset).or_default();
            *suppressed_diagnostics += 1;
            continue;
        }

        let insertion_offset = existing.insertion_offset;
        let (_, grouped_codes, grouped_diagnostics) = by_suppression
            .entry(insertion_offset)
            .or_insert_with(|| (existing, BTreeSet::new(), 0));
        grouped_codes.insert(id);
        *grouped_diagnostics += 1;
    }

    for (end_offset, (lints, suppressed_diagnostics)) in by_end {
        let codes: SmallVec<[LintName; 2]> = lints.into_iter().collect();
        fixes.push(SuppressFix {
            fix: add_end_of_line_suppression(&source, &codes, end_offset),
            suppressed_diagnostics,
        });
    }

    for (existing, codes, suppressed_diagnostics) in by_suppression.into_values() {
        let codes: SmallVec<[LintName; 2]> = codes.into_iter().collect();
        fixes.push(SuppressFix {
            fix: add_to_existing_suppression(existing, &codes),
            suppressed_diagnostics,
        });
    }

    fixes
}

/// Fix to suppress one or more diagnostics.
pub struct SuppressFix {
    pub fix: Fix,
    /// The number of diagnostics that will be suppressed if this fix is applied.
    pub suppressed_diagnostics: usize,
}

/// Creates a fix to suppress a single lint, except for suppression diagnostics in a shebang.
pub fn suppress_single(db: &dyn Db, file: File, id: LintId, range: TextRange) -> Option<Fix> {
    let suppressions = suppressions(db, file);
    let source = source_text(db, file);
    let codes = &[id.name()];

    if let Some(existing) = find_existing_suppression(suppressions, &source, range, id.name()) {
        return Some(add_to_existing_suppression(existing, codes));
    }

    if is_suppression_comment_lint(id.name()) {
        match suppression_comment_fix(db, file, range)? {
            SuppressionCommentFix::LineLocal(start) => {
                return Some(add_line_local_suppression(&source, &[id.name()], start));
            }
            SuppressionCommentFix::SameLine => {}
            SuppressionCommentFix::Shebang => return None,
        }
    }

    let suppression_range = suppression_range(db, file, range);

    Some(add_end_of_line_suppression(
        &source,
        codes,
        suppression_range.end(),
    ))
}

/// Returns whether a diagnostic can be included in a bulk suppression fix.
pub(crate) fn can_suppress(db: &dyn Db, file: File, id: LintName, range: TextRange) -> bool {
    !is_suppression_comment_lint(id) || suppression_comment_fix(db, file, range).is_some()
}

enum SuppressionCommentFix {
    LineLocal(TextSize),
    SameLine,
    Shebang,
}

/// Classifies how to suppress a diagnostic emitted for an existing ignore comment.
///
/// Own-line diagnostics get a line-local prefix, while inline and file-level diagnostics use an
/// end-of-line suppression. Shebangs remain eligible for bulk fixes but not IDE quick fixes.
///
/// ```python
/// seen_code = True
/// # ty: ignore[not-a-rule]  # receives a line-local prefix
/// ```
fn suppression_comment_fix(
    db: &dyn Db,
    file: File,
    range: TextRange,
) -> Option<SuppressionCommentFix> {
    let parsed = parsed_module(db, file).load(db);
    let tokens = parsed.tokens();
    let comment = tokens
        .at_offset(range.start())
        .find(|token| token.kind() == TokenKind::Comment)?;

    let source = source_text(db, file);
    if indentation_at_offset(comment.start(), &source).is_none() {
        return Some(SuppressionCommentFix::SameLine);
    }

    // A suppression before the first non-trivia token is file-level, so bulk fixes append a
    // ty-specific suppression on the same line instead of prepending a line-local one.
    if suppressions(db, file)
        .first_non_trivia_token
        .is_none_or(|start| comment.start() < start)
    {
        return Some(if source[comment.range()].starts_with("#!") {
            SuppressionCommentFix::Shebang
        } else {
            SuppressionCommentFix::SameLine
        });
    }

    let before_diagnostic = &source[TextRange::new(comment.start(), range.start())];
    let start = before_diagnostic
        .rfind('#')
        .map_or(comment.start(), |hash| {
            comment.start() + before_diagnostic[..hash].text_len()
        });

    Some(SuppressionCommentFix::LineLocal(start))
}

/// Adds an own-line `ty: ignore` before a diagnostic on an existing comment line.
fn add_line_local_suppression(source: &str, codes: &[LintName], start: TextSize) -> Fix {
    let line_start = source.line_start(start);
    let indentation = leading_indentation(&source[line_start.to_usize()..]);
    let line_ending = find_newline(source).map_or("\n", |(_, ending)| ending.as_str());
    let insertion = format!(
        "{indentation}# ty:ignore[{codes}]{line_ending}",
        codes = Codes(SuppressionKind::Ty, codes)
    );
    Fix::safe_edit(Edit::insertion(insertion, line_start))
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
        .unwrap_or_else(|| source_text(db, file).line_end(range.end()));

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

fn add_end_of_line_suppression(source: &str, codes: &[LintName], line_end: TextSize) -> Fix {
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

/// Returns insertion metadata for the preferred editable suppression covering `range`.
///
/// When multiple comments apply, a same-line or otherwise nested comment takes precedence over an
/// outer own-line suppression. Diagnostics about suppression comments never extend a
/// `type: ignore`, since that would affect other type checkers.
///
/// ```python
/// # ty: ignore[invalid-assignment]
/// values: tuple[int] = [missing]  # ty: ignore[]
/// ```
fn find_existing_suppression(
    suppressions: &Suppressions,
    source: &str,
    range: TextRange,
    id: LintName,
) -> Option<ExistingSuppression> {
    let suppression = select_preferred_suppression(
        suppressions
            .editable_inline_suppressions_rev(range)
            .filter(|suppression| {
                (!is_suppression_comment_lint(id) || !suppression.kind.is_type_ignore())
                    && editable_suppression_prefix(&source[suppression.comment_range]).is_some()
            }),
        range,
    )?;
    let prefix = editable_suppression_prefix(&source[suppression.comment_range])?;
    let separator = if prefix.ends_with('[') {
        ""
    } else if prefix.ends_with(',') {
        " "
    } else {
        ", "
    };

    Some(ExistingSuppression {
        insertion_offset: suppression.comment_range.start() + prefix.text_len(),
        kind: suppression.kind,
        separator,
    })
}

fn add_to_existing_suppression(existing: ExistingSuppression, codes: &[LintName]) -> Fix {
    let separator = existing.separator;
    let insertion = format!("{separator}{codes}", codes = Codes(existing.kind, codes));

    Fix::safe_edit(Edit::insertion(insertion, existing.insertion_offset))
}

/// Returns the portion of an ignore comment before its closing bracket if another code can be
/// appended to it.
///
/// ```python
/// # ty: ignore[]         # Editable
/// # ty: ignore[] reason  # Not editable
/// ```
fn editable_suppression_prefix(comment_text: &str) -> Option<&str> {
    // The parser accepts a reason after the code list, but rule codes can't contain `]`, so the
    // first `]` is the code list's closing bracket. Don't edit comments with trailing reasons.
    let (before_closing_bracket, after_closing_bracket) = comment_text.split_once(']')?;
    after_closing_bracket
        .trim()
        .is_empty()
        .then(|| before_closing_bracket.trim_end())
}

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
