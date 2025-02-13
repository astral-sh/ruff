use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::name::Name;
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{
    AnyNodeRef, Arguments, Comprehension, Expr, ExprAttribute, ExprCall, ExprContext,
    ExprEllipsisLiteral, ExprName, ExprStarred, Identifier,
};
use ruff_python_semantic::analyze::typing;
use ruff_python_semantic::{Binding, SemanticModel};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Check for calls and comprehensions that can be replaced with `chain.from_iterable()`.
///
/// ## Why is this bad?
/// `chain.from_iterable()` is more idiomatic and should be used instead.
///
/// Unlike the upstream rule, this rule does not check for calls of the form `sum(a, [])`
/// to avoid collision with [`quadratic-list-summation`][RUF017].
///
/// ## Example
///
/// ```python
/// from functools import reduce
/// from itertools import chain
/// from operator import add, concat
///
///
/// a = [(1, 2, 3), (4, 5, 6), (7, 8, 9)]
///
/// chain(*a)
/// reduce(add, a); reduce(add, a, [])
/// reduce(concat, a); reduce(concat, a, [])
///
/// (y for x in a for y in x)
/// [y for x in a for y in x]
/// {y for x in a for y in x}
/// ```
///
/// Use instead:
///
/// ```python
/// from itertools import chain
///
///
/// a = [(1, 2, 3), (4, 5, 6), (7, 8, 9)]
///
/// chain.from_iterable(a)
/// list(chain.from_iterable(a))
/// set(chain.from_iterable(a))
/// ```
///
/// ## Fix safety
/// The fix will be marked as unsafe if it might remove comments,
/// or if the unpacked argument is not known to be an immutable iterable.
///
/// ```python
/// a = [(1, 2), (3, 4)]
/// c1 = chain(*a)
/// c2 = chain.from_iterable(a)
///
/// a.append((5, 6))
/// print([*c1])  # [1, 2, 3, 4]
/// print([*c2])  # [1, 2, 3, 4, 5, 6]
/// ```
///
/// ## References
/// - [`itertools` &sect; `chain.from_iterable`](https://docs.python.org/3/library/itertools.html#itertools.chain.from_iterable)
///
/// [RUF017]: https://docs.astral.sh/ruff/rules/quadratic-list-summation/
#[derive(ViolationMetadata)]
pub(crate) struct ReimplementedChainFromIterable;

impl Violation for ReimplementedChainFromIterable {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `chain.from_iterable()` instead".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with `chain.from_iterable()`".to_string())
    }
}

/// FURB179
pub(crate) fn reimplemented_chain_from_iterable_comprehension(
    checker: &Checker,
    element: &Expr,
    comprehensions: &[Comprehension],
    original: AnyNodeRef,
) {
    let [outer, inner] = comprehensions else {
        return;
    };

    if outer.is_async || inner.is_async || !outer.ifs.is_empty() || !inner.ifs.is_empty() {
        return;
    }

    if ComparableExpr::from(element) != ComparableExpr::from(&inner.target)
        || ComparableExpr::from(&outer.target) != ComparableExpr::from(&inner.iter)
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(ReimplementedChainFromIterable, original.range());

    diagnostic.try_set_optional_fix(|| {
        replace_with_chain_from_iterable(original, &outer.target, checker)
    });

    checker.report_diagnostic(diagnostic);
}

/// FURB179
pub(crate) fn reimplemented_chain_from_iterable_call(checker: &Checker, call: &ExprCall) {
    let (func, arguments) = (&*call.func, &call.arguments);
    let semantic = checker.semantic();

    if !arguments.keywords.is_empty() {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return;
    };

    let diagnostic = match qualified_name.segments() {
        ["itertools", "chain"] => {
            let [argument @ Expr::Starred(ExprStarred { value, .. })] = &arguments.args[..] else {
                return;
            };

            let fix = add_from_iterable(arguments.start(), argument, value, semantic);

            Diagnostic::new(ReimplementedChainFromIterable, argument.range()).with_fix(fix)
        }

        ["functools", "reduce"] => {
            let (op, iterable) = match &arguments.args[..] {
                [op, iterable] => (op, iterable),
                [op, iterable, Expr::List(list)] if list.elts.is_empty() => (op, iterable),
                _ => return,
            };

            let Some(qualified_name) = semantic.resolve_qualified_name(op) else {
                return;
            };

            if !matches!(qualified_name.segments(), ["operator", "add" | "concat"]) {
                return;
            }

            let mut diagnostic = Diagnostic::new(ReimplementedChainFromIterable, call.range);

            diagnostic.try_set_optional_fix(|| {
                replace_with_chain_from_iterable(call.into(), iterable, checker)
            });

            diagnostic
        }

        _ => return,
    };

    checker.report_diagnostic(diagnostic);
}

fn add_from_iterable(
    position: TextSize,
    argument: &Expr,
    iterable: &Expr,
    semantic: &SemanticModel,
) -> Fix {
    let add_from_iterable = Edit::insertion(".from_iterable".to_string(), position);

    let before_star = argument.start();
    let remove_star = Edit::deletion(before_star, before_star + "*".text_len());

    let applicability = applicability_based_on_iterable_type(iterable, semantic);

    Fix::applicable_edits(add_from_iterable, [remove_star], applicability)
}

fn replace_with_chain_from_iterable(
    to_be_replaced: AnyNodeRef,
    iterable: &Expr,
    checker: &Checker,
) -> anyhow::Result<Option<Fix>> {
    let semantic = checker.semantic();
    let comment_ranges = checker.comment_ranges();
    let source = checker.source();

    let importer = checker.importer();
    let (import_chain, chain_binding) = importer.get_or_import_symbol(
        &ImportRequest::import_from("itertools", "chain"),
        to_be_replaced.start(),
        semantic,
    )?;

    let iterable_full_range =
        parenthesized_range(iterable.into(), to_be_replaced, comment_ranges, source);
    let iterable_in_source = &source[iterable_full_range.unwrap_or(iterable.range())];

    let new_call = Expr::Call(ExprCall {
        func: Box::new(Expr::Attribute(ExprAttribute {
            value: Box::new(Expr::Name(ExprName {
                id: Name::from(chain_binding),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            })),
            attr: Identifier {
                id: Name::new("from_iterable".to_string()),
                range: TextRange::default(),
            },
            ctx: ExprContext::Load,
            range: TextRange::default(),
        })),
        arguments: Arguments {
            args: Box::new([ExprEllipsisLiteral::default().into()]),
            keywords: Box::new([]),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    });
    let new_content = checker
        .generator()
        .expr(&new_call)
        .replace("...", iterable_in_source);

    let replace = Edit::range_replacement(new_content, to_be_replaced.range());

    let applicability = if comment_ranges.intersects(to_be_replaced.range()) {
        Applicability::Unsafe
    } else {
        applicability_based_on_iterable_type(iterable, semantic)
    };

    Ok(Some(Fix::applicable_edits(
        replace,
        [import_chain],
        applicability,
    )))
}

fn applicability_based_on_iterable_type(
    iterable: &Expr,
    semantic: &SemanticModel,
) -> Applicability {
    match iterable {
        _ if iterable.is_literal_expr() => Applicability::Safe,

        Expr::Name(name) => match semantic.only_binding(name).map(|id| semantic.binding(id)) {
            None => Applicability::Unsafe,
            Some(binding) => {
                if is_of_immutable_iterable_type(binding, semantic) {
                    Applicability::Safe
                } else {
                    Applicability::Unsafe
                }
            }
        },

        _ => Applicability::Unsafe,
    }
}

fn is_of_immutable_iterable_type(binding: &Binding, semantic: &SemanticModel) -> bool {
    typing::is_string(binding, semantic)
        || typing::is_bytes(binding, semantic)
        || typing::is_tuple(binding, semantic)
}
