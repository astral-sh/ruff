use ruff_python_parser::lexer::LexResult;
use ruff_python_parser::Tok;
use ruff_text_size::TextRange;

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_index::Indexer;
use ruff_source_file::Locator;

use crate::registry::Rule;
use crate::settings::Settings;

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

impl AlwaysAutofixableViolation for UselessSemicolon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Statement ends with an unnecessary semicolon")
    }

    fn autofix_title(&self) -> String {
        format!("Remove unnecessary semicolon")
    }
}

/// E701, E702, E703
pub(crate) fn compound_statements(
    diagnostics: &mut Vec<Diagnostic>,
    lxr: &[LexResult],
    locator: &Locator,
    indexer: &Indexer,
    settings: &Settings,
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

    // Track the bracket depth.
    let mut par_count = 0u32;
    let mut sqb_count = 0u32;
    let mut brace_count = 0u32;

    for &(ref tok, range) in lxr.iter().flatten() {
        match tok {
            Tok::Lpar => {
                par_count = par_count.saturating_add(1);
            }
            Tok::Rpar => {
                par_count = par_count.saturating_sub(1);
            }
            Tok::Lsqb => {
                sqb_count = sqb_count.saturating_add(1);
            }
            Tok::Rsqb => {
                sqb_count = sqb_count.saturating_sub(1);
            }
            Tok::Lbrace => {
                brace_count = brace_count.saturating_add(1);
            }
            Tok::Rbrace => {
                brace_count = brace_count.saturating_sub(1);
            }
            Tok::Ellipsis => {
                if allow_ellipsis {
                    allow_ellipsis = false;
                    continue;
                }
            }
            _ => {}
        }

        if par_count > 0 || sqb_count > 0 || brace_count > 0 {
            continue;
        }

        match tok {
            Tok::Newline => {
                if let Some((start, end)) = semi {
                    let mut diagnostic =
                        Diagnostic::new(UselessSemicolon, TextRange::new(start, end));
                    if settings.rules.should_fix(Rule::UselessSemicolon) {
                        diagnostic.set_fix(Fix::automatic(Edit::deletion(
                            indexer
                                .preceded_by_continuations(start, locator)
                                .unwrap_or(start),
                            end,
                        )));
                    };
                    diagnostics.push(diagnostic);
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
            Tok::Colon => {
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
                    colon = Some((range.start(), range.end()));

                    // Allow `class C: ...`-style definitions in stubs.
                    allow_ellipsis = class.is_some();
                }
            }
            Tok::Semi => {
                semi = Some((range.start(), range.end()));
            }
            Tok::Comment(..) | Tok::Indent | Tok::Dedent | Tok::NonLogicalNewline => {}
            _ => {
                if let Some((start, end)) = semi {
                    diagnostics.push(Diagnostic::new(
                        MultipleStatementsOnOneLineSemicolon,
                        TextRange::new(start, end),
                    ));

                    // Reset.
                    semi = None;
                }

                if let Some((start, end)) = colon {
                    diagnostics.push(Diagnostic::new(
                        MultipleStatementsOnOneLineColon,
                        TextRange::new(start, end),
                    ));

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
            }
        }

        match tok {
            Tok::Lambda => {
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
            Tok::Case => {
                case = Some((range.start(), range.end()));
            }
            Tok::If => {
                if_ = Some((range.start(), range.end()));
            }
            Tok::While => {
                while_ = Some((range.start(), range.end()));
            }
            Tok::For => {
                for_ = Some((range.start(), range.end()));
            }
            Tok::Try => {
                try_ = Some((range.start(), range.end()));
            }
            Tok::Except => {
                except = Some((range.start(), range.end()));
            }
            Tok::Finally => {
                finally = Some((range.start(), range.end()));
            }
            Tok::Elif => {
                elif = Some((range.start(), range.end()));
            }
            Tok::Else => {
                else_ = Some((range.start(), range.end()));
            }
            Tok::Class => {
                class = Some((range.start(), range.end()));
            }
            Tok::With => {
                with = Some((range.start(), range.end()));
            }
            Tok::Match => {
                match_ = Some((range.start(), range.end()));
            }
            _ => {}
        };
    }
}
