use std::slice::Iter;

use ruff_notebook::CellOffsets;
use ruff_python_ast::PySourceType;
use ruff_python_parser::{Token, TokenKind, Tokens};
use ruff_text_size::{Ranged, TextSize};

use ruff_diagnostics::{AlwaysFixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;

/// ## What it does
/// Checks for compound statements (multiple statements on the same line).
///
/// ## Why is this bad?
/// According to [PEP 8], "compound statements are generally discouraged".
///
/// ## Example
/// ```python
/// if foo == "blah": do_blah_thing()
/// ```
///
/// Use instead:
/// ```python
/// if foo == "blah":
///     do_blah_thing()
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#other-recommendations
#[violation]
pub struct MultipleStatementsOnOneLineColon;

impl Violation for MultipleStatementsOnOneLineColon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple statements on one line (colon)")
    }
}

/// ## What it does
/// Checks for multiline statements on one line.
///
/// ## Why is this bad?
/// According to [PEP 8], including multi-clause statements on the same line is
/// discouraged.
///
/// ## Example
/// ```python
/// do_one(); do_two(); do_three()
/// ```
///
/// Use instead:
/// ```python
/// do_one()
/// do_two()
/// do_three()
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#other-recommendations
#[violation]
pub struct MultipleStatementsOnOneLineSemicolon;

impl Violation for MultipleStatementsOnOneLineSemicolon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple statements on one line (semicolon)")
    }
}

/// ## What it does
/// Checks for statements that end with an unnecessary semicolon.
///
/// ## Why is this bad?
/// A trailing semicolon is unnecessary and should be removed.
///
/// ## Example
/// ```python
/// do_four();  # useless semicolon
/// ```
///
/// Use instead:
/// ```python
/// do_four()
/// ```
#[violation]
pub struct UselessSemicolon;

impl AlwaysFixableViolation for UselessSemicolon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Statement ends with an unnecessary semicolon")
    }

    fn fix_title(&self) -> String {
        format!("Remove unnecessary semicolon")
    }
}

/// E701, E702, E703
pub(crate) fn compound_statements(
    diagnostics: &mut Vec<Diagnostic>,
    tokens: &Tokens,
    locator: &Locator,
    indexer: &Indexer,
    source_type: PySourceType,
    cell_offsets: Option<&CellOffsets>,
) {
    // Track the last seen instance of a variety of tokens.
    let mut colon = None;
    let mut semi = None;
    let mut case = None;
    let mut class = None;
    let mut elif = None;
    let mut else_ = None;
    let mut except = None;
    let mut finally = None;
    let mut for_ = None;
    let mut if_ = None;
    let mut match_ = None;
    let mut try_ = None;
    let mut while_ = None;
    let mut with = None;

    // As a special-case, track whether we're at the first token after a colon.
    // This is used to allow `class C: ...`-style definitions in stubs.
    let mut allow_ellipsis = false;

    // Track the nesting level.
    let mut nesting = 0u32;

    // Track indentation.
    let mut indent = 0u32;

    // Use an iterator to allow passing it around.
    let mut token_iter = tokens.up_to_first_unknown().iter();

    loop {
        let Some(token) = token_iter.next() else {
            break;
        };

        match token.kind() {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => {
                nesting = nesting.saturating_add(1);
            }
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                nesting = nesting.saturating_sub(1);
            }
            TokenKind::Ellipsis => {
                if allow_ellipsis {
                    allow_ellipsis = false;
                    continue;
                }
            }
            TokenKind::Indent => {
                indent = indent.saturating_add(1);
            }
            TokenKind::Dedent => {
                indent = indent.saturating_sub(1);
            }
            _ => {}
        }

        if nesting > 0 {
            continue;
        }

        match token.kind() {
            TokenKind::Newline => {
                if let Some(range) = semi {
                    if !(source_type.is_ipynb()
                        && indent == 0
                        && cell_offsets
                            .and_then(|cell_offsets| cell_offsets.containing_range(token.start()))
                            .is_some_and(|cell_range| {
                                !has_non_trivia_tokens_till(token_iter.clone(), cell_range.end())
                            }))
                    {
                        let mut diagnostic = Diagnostic::new(UselessSemicolon, range);
                        diagnostic.set_fix(Fix::safe_edit(Edit::deletion(
                            indexer
                                .preceded_by_continuations(range.start(), locator)
                                .unwrap_or(range.start()),
                            range.end(),
                        )));
                        diagnostics.push(diagnostic);
                    }
                }

                // Reset.
                colon = None;
                semi = None;
                case = None;
                class = None;
                elif = None;
                else_ = None;
                except = None;
                finally = None;
                for_ = None;
                if_ = None;
                match_ = None;
                try_ = None;
                while_ = None;
                with = None;
            }
            TokenKind::Colon => {
                if case.is_some()
                    || class.is_some()
                    || elif.is_some()
                    || else_.is_some()
                    || except.is_some()
                    || finally.is_some()
                    || for_.is_some()
                    || if_.is_some()
                    || match_.is_some()
                    || try_.is_some()
                    || while_.is_some()
                    || with.is_some()
                {
                    colon = Some(token.range());

                    // Allow `class C: ...`-style definitions.
                    allow_ellipsis = true;
                }
            }
            TokenKind::Semi => {
                semi = Some(token.range());
                allow_ellipsis = false;
            }
            TokenKind::Comment
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::NonLogicalNewline => {}
            _ => {
                if let Some(range) = semi {
                    diagnostics.push(Diagnostic::new(MultipleStatementsOnOneLineSemicolon, range));

                    // Reset.
                    semi = None;
                    allow_ellipsis = false;
                }

                if let Some(range) = colon {
                    diagnostics.push(Diagnostic::new(MultipleStatementsOnOneLineColon, range));

                    // Reset.
                    colon = None;
                    case = None;
                    class = None;
                    elif = None;
                    else_ = None;
                    except = None;
                    finally = None;
                    for_ = None;
                    if_ = None;
                    match_ = None;
                    try_ = None;
                    while_ = None;
                    with = None;
                    allow_ellipsis = false;
                }
            }
        }

        match token.kind() {
            TokenKind::Lambda => {
                // Reset.
                colon = None;
                case = None;
                class = None;
                elif = None;
                else_ = None;
                except = None;
                finally = None;
                for_ = None;
                if_ = None;
                match_ = None;
                try_ = None;
                while_ = None;
                with = None;
            }
            TokenKind::Case => {
                case = Some(token.range());
            }
            TokenKind::If => {
                if_ = Some(token.range());
            }
            TokenKind::While => {
                while_ = Some(token.range());
            }
            TokenKind::For => {
                for_ = Some(token.range());
            }
            TokenKind::Try => {
                try_ = Some(token.range());
            }
            TokenKind::Except => {
                except = Some(token.range());
            }
            TokenKind::Finally => {
                finally = Some(token.range());
            }
            TokenKind::Elif => {
                elif = Some(token.range());
            }
            TokenKind::Else => {
                else_ = Some(token.range());
            }
            TokenKind::Class => {
                class = Some(token.range());
            }
            TokenKind::With => {
                with = Some(token.range());
            }
            TokenKind::Match => {
                match_ = Some(token.range());
            }
            _ => {}
        };
    }
}

/// Returns `true` if there are any non-trivia tokens from the given token
/// iterator till the given end offset.
fn has_non_trivia_tokens_till(tokens: Iter<'_, Token>, cell_end: TextSize) -> bool {
    for token in tokens {
        if token.start() >= cell_end {
            return false;
        }
        if !matches!(
            token.kind(),
            TokenKind::Newline
                | TokenKind::Comment
                | TokenKind::EndOfFile
                | TokenKind::NonLogicalNewline
        ) {
            return true;
        }
    }
    false
}
