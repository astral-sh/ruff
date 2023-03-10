use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use ruff_diagnostics::{AlwaysAutofixableViolation, Violation};
use ruff_diagnostics::{Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::registry::Rule;
use crate::settings::{flags, Settings};

/// ## What it does
/// Checks for multiline statements on one line.
///
/// ## Why is this bad?
/// Compound statements (on the same line) are generally
/// discouraged.
///
/// While sometimes it's okay to put an if/for/while with a small body
/// on the same line, never do this for multi-clause statements.
/// Also avoid folding such long lines!
///
/// ## Example
/// ```python
/// if foo == 'blah': do_blah_thing()
/// for x in lst: total += x
/// while t < 10: t = delay()
/// if foo == 'blah': do_blah_thing()
/// else: do_non_blah_thing()
/// try: something()
/// finally: cleanup()
/// if foo == 'blah': one(); two(); three()
///
/// ```
///
/// Use instead:
/// ```python
/// if foo == 'blah':
///     do_blah_thing()
/// ```
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
/// Compound statements (on the same line) are generally
/// discouraged.
///
/// While sometimes it's okay to put an if/for/while with a small body
/// on the same line, never do this for multi-clause statements.
/// Also avoid folding such long lines!
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
#[violation]
pub struct MultipleStatementsOnOneLineSemicolon;

impl Violation for MultipleStatementsOnOneLineSemicolon {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Multiple statements on one line (semicolon)")
    }
}

/// ## What it does
/// Checks for statements that end with a semicolon.
///
/// ## Why is this bad?
///
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
pub fn compound_statements(
    lxr: &[LexResult],
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];

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
    let mut par_count = 0;
    let mut sqb_count = 0;
    let mut brace_count = 0;

    for &(start, ref tok, end) in lxr.iter().flatten() {
        match tok {
            Tok::Lpar => {
                par_count += 1;
            }
            Tok::Rpar => {
                par_count -= 1;
            }
            Tok::Lsqb => {
                sqb_count += 1;
            }
            Tok::Rsqb => {
                sqb_count -= 1;
            }
            Tok::Lbrace => {
                brace_count += 1;
            }
            Tok::Rbrace => {
                brace_count -= 1;
            }
            _ => {}
        }

        if par_count > 0 || sqb_count > 0 || brace_count > 0 {
            continue;
        }

        match tok {
            Tok::Newline => {
                if let Some((start, end)) = semi {
                    let mut diagnostic = Diagnostic::new(UselessSemicolon, Range::new(start, end));
                    if autofix.into() && settings.rules.should_fix(&Rule::UselessSemicolon) {
                        diagnostic.amend(Fix::deletion(start, end));
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
                    colon = Some((start, end));
                    allow_ellipsis = true;
                }
            }
            Tok::Semi => {
                semi = Some((start, end));
            }
            Tok::Comment(..) | Tok::Indent | Tok::Dedent | Tok::NonLogicalNewline => {}
            Tok::Ellipsis if allow_ellipsis => {
                // Allow `class C: ...`-style definitions in stubs.
                allow_ellipsis = false;
            }
            _ => {
                if let Some((start, end)) = semi {
                    diagnostics.push(Diagnostic::new(
                        MultipleStatementsOnOneLineSemicolon,
                        Range::new(start, end),
                    ));

                    // Reset.
                    semi = None;
                }

                if let Some((start, end)) = colon {
                    diagnostics.push(Diagnostic::new(
                        MultipleStatementsOnOneLineColon,
                        Range::new(start, end),
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
                case = Some((start, end));
            }
            Tok::If => {
                if_ = Some((start, end));
            }
            Tok::While => {
                while_ = Some((start, end));
            }
            Tok::For => {
                for_ = Some((start, end));
            }
            Tok::Try => {
                try_ = Some((start, end));
            }
            Tok::Except => {
                except = Some((start, end));
            }
            Tok::Finally => {
                finally = Some((start, end));
            }
            Tok::Elif => {
                elif = Some((start, end));
            }
            Tok::Else => {
                else_ = Some((start, end));
            }
            Tok::Class => {
                class = Some((start, end));
            }
            Tok::With => {
                with = Some((start, end));
            }
            Tok::Match => {
                match_ = Some((start, end));
            }
            _ => {}
        };
    }

    diagnostics
}
