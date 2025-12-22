use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use ruff_db::diagnostic::LintName;
use ruff_db::files::File;
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::token::TokenKind;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::Db;
use crate::lint::LintId;
use crate::suppression::{SuppressionTarget, suppressions};

pub fn suppress_all<I>(db: &dyn Db, file: File, ids_with_range: I) -> Vec<Fix>
where
    I: IntoIterator<Item = (LintName, TextRange)>,
{
    let grouped = group_by_suppression_range(db, file, ids_with_range);
    create_all_fixes(db, file, grouped)
}

/// Creates a fix to suppress a single lint.
pub fn suppress_single(db: &dyn Db, file: File, id: LintId, range: TextRange) -> Fix {
    let suppression_range = suppression_range(db, file, range);
    create_suppression_fix(db, file, id.name(), suppression_range)
}

fn create_all_fixes(
    db: &dyn Db,
    file: File,
    grouped: BTreeMap<SuppressionRange, BTreeSet<LintName>>,
) -> Vec<Fix> {
    let mut fixes = Vec::new();

    for (range, lints) in grouped {
        for lint in lints.into_iter().rev() {
            let fix = create_suppression_fix(db, file, lint, range);
            fixes.push(fix);
        }
    }

    fixes
}

fn group_by_suppression_range<I>(
    db: &dyn Db,
    file: File,
    ids_with_range: I,
) -> BTreeMap<SuppressionRange, BTreeSet<LintName>>
where
    I: IntoIterator<Item = (LintName, TextRange)>,
{
    let mut map: BTreeMap<SuppressionRange, BTreeSet<LintName>> = BTreeMap::new();
    for (id, range) in ids_with_range {
        let full_range = suppression_range(db, file, range);
        map.entry(full_range).or_default().insert(id);
    }

    map
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
fn suppression_range(db: &dyn Db, file: File, range: TextRange) -> SuppressionRange {
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

    SuppressionRange(TextRange::new(line_start, line_end))
}

/// The range of the suppression.
///
/// Guaranteed to start at the start of a line and
/// ends at the end of a line (right before the `\n`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SuppressionRange(TextRange);

impl SuppressionRange {
    fn text_range(&self) -> TextRange {
        self.0
    }

    fn line_end(&self) -> TextSize {
        self.0.end()
    }
}

impl PartialOrd for SuppressionRange {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SuppressionRange {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.ordering(other.0)
    }
}

/// Creates a fix for adding a suppression comment to suppress `lint` for `range`.
///
/// The fix prefers adding the code to an existing `ty: ignore[]` comment over
/// adding a new suppression comment.
fn create_suppression_fix(
    db: &dyn Db,
    file: File,
    name: LintName,
    suppression_range: SuppressionRange,
) -> Fix {
    let suppressions = suppressions(db, file);
    let source = source_text(db, file);

    let mut existing_suppressions = suppressions
        .line_suppressions(suppression_range.text_range())
        .filter(|suppression| {
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
                format!(" {name}")
            } else {
                format!(", {name}")
            };

            let relative_offset_from_end = comment_text.text_len() - up_to_last_code.text_len();

            return Fix::safe_edit(Edit::insertion(
                insertion,
                existing.comment_range.end() - relative_offset_from_end,
            ));
        }
    }

    // Always insert a new suppression at the end of the range to avoid having to deal with multiline strings
    // etc.

    let line_end = suppression_range.line_end();
    let up_to_line_end = &source[..line_end.to_usize()];
    let up_to_first_content = up_to_line_end.trim_end();
    let trailing_whitespace_len = up_to_line_end.text_len() - up_to_first_content.text_len();

    let insertion = format!("  # ty:ignore[{name}]");

    Fix::safe_edit(if trailing_whitespace_len == TextSize::ZERO {
        Edit::insertion(insertion, line_end)
    } else {
        // `expr # fmt: off<trailing_whitespace>`
        // Trim the trailing whitespace
        Edit::replacement(insertion, line_end - trailing_whitespace_len, line_end)
    })
}
