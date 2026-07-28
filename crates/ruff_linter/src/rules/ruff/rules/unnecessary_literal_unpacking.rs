use ruff_diagnostics::IsolationLevel;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::{Token, TokenKind};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::fix::edits::{Parentheses, remove_argument, remove_member};
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for iterable unpacking (`*`) of a list or tuple literal, or of a single-element set
/// literal, in a position where the literal's elements could be written directly.
///
/// ## Why is this bad?
/// The literal is built only to be taken apart again on the very next step. Writing its
/// elements directly is shorter, avoids allocating a throwaway collection, and keeps the type of
/// each individual element visible to type checkers.
///
/// A set literal is only reported when it holds exactly one element written out in full, as in
/// `foo(*{bar})`, which is the case where the set can never do anything. Unpacking any other set
/// literal is left alone, because the set is doing real work: `foo(*{bar, baz})` and
/// `foo(*{*rest})` both deduplicate.
///
/// ## Example
/// ```python
/// foo(*[bar])
/// foo(*(bar, *rest))
/// values = [*[bar, baz], qux]
/// ```
///
/// Use instead:
/// ```python
/// foo(bar)
/// foo(bar, *rest)
/// values = [bar, baz, qux]
/// ```
///
/// ## Fix safety
/// The fix is marked unsafe when:
///
/// - The literal is a **set**. `foo(*{bar})` is not equivalent to `foo(bar)`: building the set
///   requires `bar` to be hashable, so the fix can silence a `TypeError` that is most likely a real
///   mistake.
/// - A comment sits in text the fix deletes: between the `*` and the literal, in the gap that
///   removing an empty unpacking takes along with the neighbouring comma, or inside a collection
///   that the fix rewrites wholesale. The comment does not survive the fix.
///
/// ## Fix availability
/// No fix is offered at all where writing the elements out would not be valid Python. Neither case
/// applies to an empty literal, which is removed whole rather than written out:
///
/// - A keyword argument written before the unpacking, since neither `foo(keyword=bar, baz)` nor
///   `class C(metaclass=Meta, Base)` is valid Python.
/// - A multi-line literal inside a tuple written without parentheses, where the literal's
///   brackets are what continue the lines.
///
/// ## See also
/// [`unnecessary-spread`][PIE800] is the counterpart for the dictionary unpacking operator (`**`).
///
/// ## References
/// - [PEP 448 – Additional Unpacking Generalizations](https://peps.python.org/pep-0448/)
/// - [Python documentation: Expression lists](https://docs.python.org/3/reference/expressions.html#expression-lists)
/// - [Python documentation: Calls](https://docs.python.org/3/reference/expressions.html#calls)
///
/// [PIE800]: https://docs.astral.sh/ruff/rules/unnecessary-spread/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct UnnecessaryLiteralUnpacking {
    kind: SequenceKind,
}

impl Violation for UnnecessaryLiteralUnpacking {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.kind {
            SequenceKind::List => "Unnecessary unpacking of list literal".to_string(),
            SequenceKind::Tuple => "Unnecessary unpacking of tuple literal".to_string(),
            SequenceKind::Set => "Unnecessary unpacking of set literal".to_string(),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let title = match self.kind {
            SequenceKind::List => "Remove unnecessary list",
            SequenceKind::Tuple => "Remove unnecessary tuple",
            SequenceKind::Set => "Remove unnecessary set",
        };
        Some(title.to_string())
    }
}

/// RUF077
pub(crate) fn unnecessary_literal_unpacking(checker: &Checker, starred: &ast::ExprStarred) {
    // A starred expression in a store context is an assignment target (`a, *b = c`), not an
    // unpacking.
    if !starred.ctx.is_load() {
        return;
    }

    let Some(literal) = SequenceLiteral::from_expr(&starred.value) else {
        return;
    };

    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryLiteralUnpacking { kind: literal.kind },
        starred.range(),
    );

    if let Some(fix) = unnecessary_literal_unpacking_fix(checker, starred, &literal) {
        diagnostic.set_fix(fix);
    }
}

/// Build a fix that drops the `*` and the literal's brackets, leaving its elements in place.
///
/// Returns `None` where dropping the brackets would not round-trip: either the result would not be
/// valid Python, which the `## Fix safety` section of the rule documentation lists, or the `*` and
/// the literal are separated by something other than redundant parentheses, which this rule makes no
/// attempt to understand.
fn unnecessary_literal_unpacking_fix(
    checker: &Checker,
    starred: &ast::ExprStarred,
    literal: &SequenceLiteral,
) -> Option<Fix> {
    // Redundant parentheses may sit between the `*` and the literal, as in `foo(*([bar, baz]))`.
    // They have to go along with the brackets, or they would turn the expanded elements back into
    // a single tuple argument. Anything else in that gap means the `*` does not apply to the
    // literal in a shape this rule understands.
    let before_literal = TextRange::new(starred.start(), literal.start());
    let mut kinds = checker
        .tokens()
        .in_range(before_literal)
        .iter()
        .map(Token::kind)
        .filter(|kind| !kind.is_trivia());
    if kinds.next() != Some(TokenKind::Star) {
        return None;
    }
    let mut redundant_parens = 0usize;
    for kind in kinds {
        if kind != TokenKind::Lpar {
            return None;
        }
        redundant_parens += 1;
    }

    let parent = checker.semantic().current_expression_parent();
    // A call keeps its arguments on the `ExprCall` above the unpacking, but the base list of a
    // class definition has no enclosing expression at all, so it has to be read off the statement.
    // The range check makes sure the statement's arguments really are the ones being expanded into.
    let arguments = match parent {
        Some(Expr::Call(call)) => Some(&call.arguments),
        None if let Stmt::ClassDef(class_def) = checker.semantic().current_statement() => class_def
            .arguments
            .as_deref()
            .filter(|arguments| arguments.range().contains_range(starred.range())),
        _ => None,
    };

    let Some(last_element) = literal.elts.last() else {
        return empty_literal_fix(checker, starred, literal.kind, parent, arguments);
    };

    // An unparenthesized tuple has no brackets of its own, so the literal's brackets are what let
    // it span several lines. Removing them would leave the continuation lines unterminated.
    if matches!(parent, Some(Expr::Tuple(tuple)) if !tuple.parenthesized)
        && checker
            .locator()
            .slice(starred.range())
            .contains(['\n', '\r'])
    {
        return None;
    }

    // Expanding must not move a positional argument after a keyword argument: neither
    // `foo(keyword=bar, baz)` nor `class C(metaclass=Meta, Base)` is valid Python.
    if let Some(arguments) = arguments
        && arguments
            .keywords
            .iter()
            .any(|keyword| keyword.start() < starred.start())
    {
        return None;
    }

    // Remove the `*` and the opening bracket.
    let open_bracket_edit = Edit::range_deletion(TextRange::new(
        starred.start(),
        literal.start() + TextSize::from(1),
    ));

    // A trailing comma inside the literal would end up next to the comma that separates the
    // literal from its neighbours: `[*[a,], b]` must not become `[a,, b]`.
    let comma_edit = checker
        .tokens()
        .in_range(TextRange::new(last_element.end(), literal.end()))
        .iter()
        .find(|token| token.kind() == TokenKind::Comma)
        .map(|comma| Edit::range_deletion(comma.range()));

    // `A[*Ts]` subscripts `A` with a one-element tuple even though it contains no comma: it is a
    // tuple purely because of the `*`. Expanding a single element there would make `A[*(int,)]`
    // into `A[int]`, which subscripts with `int` rather than with `(int,)`, so replace the closing
    // bracket with the comma that the tuple now needs. Two or more elements bring their own comma.
    let close_bracket = TextRange::new(literal.end() - TextSize::from(1), literal.end());
    let close_bracket_edit = if literal.elts.len() == 1
        && matches!(parent, Some(Expr::Tuple(tuple)) if !tuple.parenthesized && tuple.range() == starred.range())
    {
        Edit::range_replacement(",".to_string(), close_bracket)
    } else {
        Edit::range_deletion(close_bracket)
    };

    // Each redundant opening parenthesis swallowed above has a closing one waiting after the
    // literal. Comments are left where they are, since only the parentheses themselves go.
    let mut paren_edits = Vec::with_capacity(redundant_parens);
    for token in checker.tokens().after(literal.end()) {
        if paren_edits.len() == redundant_parens {
            break;
        }
        if token.kind().is_trivia() {
            continue;
        }
        if token.kind() != TokenKind::Rpar {
            return None;
        }
        paren_edits.push(Edit::range_deletion(token.range()));
    }
    if paren_edits.len() != redundant_parens {
        return None;
    }

    let edits = std::iter::once(open_bracket_edit)
        .chain(comma_edit)
        .chain([close_bracket_edit])
        .chain(paren_edits)
        .collect();

    build_fix(checker, literal.kind, edits, IsolationLevel::default())
}

/// Build a fix for unpacking an empty literal, as in `f(*[])`, which contributes no elements at
/// all and so has to be removed together with one of the commas around it.
///
/// Every fix built here is isolated, so the fixer applies at most one of them per pass. That lets
/// each one reason about a display losing a single element: `(*[], bar, *[])` shrinks to
/// `(bar, *[])` and only then to `(bar,)`, instead of losing both unpackings at once and
/// collapsing to plain `bar`.
fn empty_literal_fix(
    checker: &Checker,
    starred: &ast::ExprStarred,
    kind: SequenceKind,
    parent: Option<&Expr>,
    arguments: Option<&ast::Arguments>,
) -> Option<Fix> {
    let position_of = |elts: &[Expr]| elts.iter().position(|elt| elt.range() == starred.range());

    let edits = if let Some(arguments) = arguments {
        vec![
            remove_argument(
                starred,
                arguments,
                Parentheses::Preserve,
                checker.source(),
                checker.tokens(),
            )
            .ok()?,
        ]
    } else {
        match parent? {
            Expr::List(ast::ExprList { elts, .. }) => {
                vec![remove_member(elts, position_of(elts)?, checker.source()).ok()?]
            }
            // A set display cannot shrink to `{}`, which is an empty dict, so an emptied one has
            // to be spelled out as a call instead.
            Expr::Set(set) if matches!(set.elts.as_slice(), [only] if only.range() == starred.range()) =>
            {
                if !checker.semantic().has_builtin_binding("set") {
                    return None;
                }
                vec![Edit::range_replacement("set()".to_string(), set.range())]
            }
            Expr::Set(ast::ExprSet { elts, .. }) => {
                vec![remove_member(elts, position_of(elts)?, checker.source()).ok()?]
            }
            // A tuple display can shrink to `()`, but deleting the unpacking alone would not get
            // there: a tuple written without parentheses keeps its comma outside the unpacking.
            Expr::Tuple(tuple) if matches!(tuple.elts.as_slice(), [only] if only.range() == starred.range()) =>
            {
                vec![Edit::range_replacement("()".to_string(), tuple.range())]
            }
            Expr::Tuple(tuple) => {
                empty_tuple_member_edits(checker, tuple, position_of(&tuple.elts)?)?
            }
            _ => return None,
        }
    };

    build_fix(
        checker,
        kind,
        edits,
        Checker::isolation(checker.semantic().current_statement_id()),
    )
}

/// Assemble `edits` into a fix, deciding how safe it is to apply.
///
/// The fix is unsafe when it would delete a comment, since the comment is gone from the source
/// afterwards, and when the literal is a set: writing out a set's element drops the hashability
/// requirement that building the set imposed, which can silence a `TypeError` that is most likely a
/// real mistake.
fn build_fix(
    checker: &Checker,
    kind: SequenceKind,
    edits: Vec<Edit>,
    isolation: IsolationLevel,
) -> Option<Fix> {
    // `comments_in_range` rather than `intersects`: a deletion that stops exactly where a comment
    // begins, as in `f(*[],  # comment`, leaves the comment alone.
    let deletes_comment = edits.iter().any(|edit| {
        !checker
            .comment_ranges()
            .comments_in_range(edit.range())
            .is_empty()
    });
    let applicability = if matches!(kind, SequenceKind::Set) || deletes_comment {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    let mut edits = edits.into_iter();
    let first = edits.next()?;
    Some(Fix::applicable_edits(first, edits, applicability).isolate(isolation))
}

/// Remove the element at `index` from a tuple display, keeping the result a tuple.
///
/// Removing an element takes one of the display's commas with it, and a tuple needs a comma to
/// stay a tuple. Shrinking to a single element therefore has to put a trailing comma back, so that
/// `(*[], bar)` becomes `(bar,)` rather than `(bar)`, which is just `bar`. A display shrinking to
/// nothing would need `()` written in place of the whole thing, which this does not attempt.
fn empty_tuple_member_edits(
    checker: &Checker,
    tuple: &ast::ExprTuple,
    index: usize,
) -> Option<Vec<Edit>> {
    let surviving = match tuple.elts.as_slice() {
        // Three or more elements leave at least two behind, so the display keeps a comma of its
        // own and needs no help putting one back.
        [_, _, _, ..] => None,
        // A display losing its only element is rewritten as `()` by the caller instead.
        [] | [_] => return None,
        [first, second] => Some(if index == 0 { second } else { first }),
    };

    let removal = remove_member(&tuple.elts, index, checker.source()).ok()?;
    let Some(surviving) = surviving else {
        return Some(vec![removal]);
    };

    // Any comma after the surviving element that the removal does not swallow already keeps the
    // display a tuple, as in `(*[], bar,)`.
    let trailing_comma_survives = checker
        .tokens()
        .in_range(TextRange::new(surviving.end(), tuple.end()))
        .iter()
        .any(|token| {
            token.kind() == TokenKind::Comma && !removal.range().contains_range(token.range())
        });

    let mut edits = vec![removal];
    if !trailing_comma_survives {
        edits.push(Edit::insertion(",".to_string(), surviving.end()));
    }
    Some(edits)
}

/// A list, set, or parenthesized tuple display appearing as the operand of a `*`.
struct SequenceLiteral<'a> {
    kind: SequenceKind,
    elts: &'a [Expr],
    range: TextRange,
}

impl<'a> SequenceLiteral<'a> {
    fn from_expr(expr: &'a Expr) -> Option<Self> {
        match expr {
            Expr::List(list) => Some(Self {
                kind: SequenceKind::List,
                elts: &list.elts,
                range: list.range(),
            }),
            // A set with two or more elements is unpacked to deduplicate them, and `{*rest}`
            // deduplicates whatever `rest` yields, so neither is a pointless literal. Only a set
            // holding a single element written out in full can never do anything.
            Expr::Set(set) => match set.elts.as_slice() {
                [element] if !element.is_starred_expr() => Some(Self {
                    kind: SequenceKind::Set,
                    elts: &set.elts,
                    range: set.range(),
                }),
                _ => None,
            },
            // An unparenthesized tuple cannot be the operand of a `*` (the comma binds less
            // tightly), and it has no brackets for a fix to remove.
            Expr::Tuple(tuple) if tuple.parenthesized => Some(Self {
                kind: SequenceKind::Tuple,
                elts: &tuple.elts,
                range: tuple.range(),
            }),
            _ => None,
        }
    }
}

impl Ranged for SequenceLiteral<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

#[derive(Debug, Clone, Copy)]
enum SequenceKind {
    List,
    Tuple,
    Set,
}
