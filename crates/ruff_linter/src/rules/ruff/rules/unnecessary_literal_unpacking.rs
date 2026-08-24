use std::fmt::{Display, Formatter};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
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
/// - A comment sits between the `*` and the literal, in text that the fix deletes. The comment does
///   not survive the fix.
///
/// ## Fix availability
/// No fix is offered where writing the elements out would not be valid Python:
///
/// - A keyword argument written before the unpacking, since neither `foo(keyword=bar, baz)` nor
///   `class C(metaclass=Meta, Base)` is valid Python.
/// - A multi-line literal inside a tuple written without parentheses, where the literal's
///   brackets are what continue the lines. A subscript slice is written that way too, but the
///   subscript's own brackets continue the lines there, so it is still fixed.
///
/// An empty literal, as in `foo(*[])`, is reported but not fixed.
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
        let UnnecessaryLiteralUnpacking { kind } = self;
        format!("Unnecessary unpacking of {kind} literal")
    }

    fn fix_title(&self) -> Option<String> {
        let UnnecessaryLiteralUnpacking { kind } = self;
        Some(format!("Remove unnecessary {kind}"))
    }
}

/// PIE811
pub(crate) fn unnecessary_literal_unpacking(checker: &Checker, starred: &ast::ExprStarred) {
    // A starred expression in a store context is an assignment target (`a, *b = c`), not an
    // unpacking.
    if !starred.ctx.is_load() {
        return;
    }

    let Some(literal) = SequenceLiteral::from_expr(&starred.value) else {
        return;
    };

    let Some(context) = UnpackingContext::for_starred(checker, starred) else {
        return;
    };

    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryLiteralUnpacking { kind: literal.kind },
        starred.range(),
    );

    if let Some(fix) = unnecessary_literal_unpacking_fix(checker, starred, &literal, context) {
        diagnostic.set_fix(fix);
    }
}

/// Build a fix that drops the `*` and the literal's brackets, leaving its elements in place.
///
/// Returns `None` where the literal is empty, and where dropping the brackets would not round-trip.
/// The `## Fix availability` section of the rule documentation lists both.
fn unnecessary_literal_unpacking_fix(
    checker: &Checker,
    starred: &ast::ExprStarred,
    literal: &SequenceLiteral,
    context: UnpackingContext,
) -> Option<Fix> {
    // An empty literal, as in `foo(*[])`, has no elements to write out: removing it takes a
    // neighbouring comma along with it, and can leave the surrounding collection needing to be
    // rewritten. No fix is offered for it.
    let [.., last_element] = literal.elts else {
        return None;
    };

    // A tuple written without parentheses, as in `values = *[bar], baz` or the slice of
    // `A[*(int,)]`, has no brackets of its own.
    let bare_tuple = match context {
        UnpackingContext::Display(Expr::Tuple(tuple)) if !tuple.parenthesized => Some(tuple),
        _ => None,
    };

    // The literal's brackets can be what lets such a tuple span several lines. Removing them would
    // leave the continuation lines unterminated, unless something else already brackets the tuple,
    // as a subscript does in `A[*(\n int,\n)]`.
    if bare_tuple.is_some()
        && !checker
            .semantic()
            .current_expression_grandparent()
            .is_some_and(Expr::is_subscript_expr)
        && checker
            .locator()
            .slice(starred.range())
            .contains(['\n', '\r'])
    {
        return None;
    }

    // Expanding must not move a positional argument after a keyword argument: neither
    // `foo(keyword=bar, baz)` nor `class C(metaclass=Meta, Base)` is valid Python.
    if let UnpackingContext::Arguments(arguments) = context
        && arguments
            .keywords
            .iter()
            .any(|keyword| keyword.start() < starred.start())
    {
        return None;
    }

    // Remove the `*` and the opening bracket, along with anything in between. Only redundant
    // parentheses can sit in that gap, as in `foo(*([bar, baz]))`, and they have to go along with
    // the brackets, or they would turn the expanded elements back into a single tuple argument.
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
        && bare_tuple.is_some_and(|tuple| tuple.range() == starred.range())
    {
        Edit::range_replacement(",".to_string(), close_bracket)
    } else {
        Edit::range_deletion(close_bracket)
    };

    // The closing parenthesis of each redundant pair swallowed above waits between the literal and
    // the end of the `*` expression, which the parser extends over those parentheses. Comments are
    // left where they are, since only the parentheses themselves go.
    let paren_edits = checker
        .tokens()
        .in_range(TextRange::new(literal.end(), starred.end()))
        .iter()
        .filter(|token| token.kind() == TokenKind::Rpar)
        .map(|token| Edit::range_deletion(token.range()));

    let rest = comma_edit
        .into_iter()
        .chain([close_bracket_edit])
        .chain(paren_edits)
        .collect();

    Some(build_fix(checker, literal.kind, open_bracket_edit, rest))
}

/// Assemble `first` and `rest` into a fix, deciding how safe it is to apply.
///
/// The fix is unsafe when it would delete a comment, since the comment is gone from the source
/// afterwards, and when the literal is a set: writing out a set's element drops the hashability
/// requirement that building the set imposed, which can silence a `TypeError` that is most likely a
/// real mistake.
fn build_fix(checker: &Checker, kind: SequenceKind, first: Edit, rest: Vec<Edit>) -> Fix {
    // `comments_in_range` rather than `intersects`: a deletion that stops exactly where a comment
    // begins, as with the parenthesis deleted in `f(*([bar]  # comment`, leaves the comment alone.
    let deletes_comment = std::iter::once(&first).chain(&rest).any(|edit| {
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

    Fix::applicable_edits(first, rest, applicability)
}

/// The place a `*` unpacking sits in, which is what decides whether the unpacked literal's
/// elements can be written out where the unpacking is.
#[derive(Debug, Clone, Copy)]
enum UnpackingContext<'a> {
    /// An argument list: the arguments of a call, as in `foo(*[bar])`,
    /// or the bases of a class definition, as in `class C(*[Base]): ...`.
    Arguments(&'a ast::Arguments),
    /// An element of a list, set, or tuple display. A subscript slice counts as a tuple display:
    /// `A[*Ts]` subscripts `A` with a one-element tuple.
    Display(&'a Expr),
}

impl<'a> UnpackingContext<'a> {
    /// Classify where `starred` sits, or return `None` where its elements cannot be written out at
    /// all.
    fn for_starred(checker: &Checker<'a>, starred: &ast::ExprStarred) -> Option<Self> {
        match checker.semantic().current_expression_parent() {
            Some(Expr::Call(call)) => Some(Self::Arguments(&call.arguments)),
            Some(parent @ (Expr::List(_) | Expr::Set(_) | Expr::Tuple(_))) => {
                Some(Self::Display(parent))
            }
            // The bases of a class definition are not wrapped in an expression, so an unpacking
            // among them has no parent expression and has to be recognised from the statement. A
            // `*` also has no parent expression in the default of a type parameter, as in
            // `class C[*Ts = *(int, str)]`, where the elements cannot be written out: the default
            // of a `TypeVarTuple` has to be an unpacking, and `class C[*Ts = int, str]` would
            // declare a second type parameter instead. Checking that the bases really do contain
            // the unpacking is what tells the two apart.
            None if let Stmt::ClassDef(class_def) = checker.semantic().current_statement() => {
                class_def
                    .arguments
                    .as_deref()
                    .filter(|arguments| arguments.range().contains_range(starred.range()))
                    .map(Self::Arguments)
            }
            _ => None,
        }
    }
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

impl Display for SequenceKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SequenceKind::List => "list",
            SequenceKind::Tuple => "tuple",
            SequenceKind::Set => "set",
        })
    }
}
