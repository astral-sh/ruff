use ruff_notebook::CellOffsets;
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{TextRange, TextSize};

use crate::Locator;
use ruff_python_ast::{self as ast, Arguments, Expr};

/// Return `true` if the statement ending at `statement_end` is the last
/// top-level expression in its cell. This assumes that the source is a Jupyter
/// Notebook and that the statement is at the module's top level.
pub(crate) fn at_last_top_level_expression_in_cell(
    statement_end: TextSize,
    locator: &Locator,
    cell_offsets: Option<&CellOffsets>,
) -> bool {
    cell_offsets
        .and_then(|cell_offsets| cell_offsets.containing_range(statement_end))
        .is_some_and(|cell_range| {
            SimpleTokenizer::new(
                locator.contents(),
                TextRange::new(statement_end, cell_range.end()),
            )
            .all(|token| token.kind() == SimpleTokenKind::Semi || token.kind().is_trivia())
        })
}

/// Return `true` if the [`Expr`] appears to be an infinite iterator (e.g., a call to
/// `itertools.cycle` or similar).
pub(crate) fn is_infinite_iterable(arg: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, keywords, .. },
        ..
    }) = &arg
    else {
        return false;
    };

    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| match qualified_name.segments() {
            ["itertools", "cycle" | "count"] => true,
            ["itertools", "repeat"] => {
                // Ex) `itertools.repeat(1)`
                if keywords.is_empty() && args.len() == 1 {
                    return true;
                }

                // Ex) `itertools.repeat(1, None)`
                if args.len() == 2 && args[1].is_none_literal_expr() {
                    return true;
                }

                // Ex) `itertools.repeat(1, times=None)`
                for keyword in keywords {
                    if keyword.arg.as_ref().is_some_and(|name| name == "times")
                        && keyword.value.is_none_literal_expr()
                    {
                        return true;
                    }
                }

                false
            }
            _ => false,
        })
}

/// Return `true` if any expression in the iterator appears to be an infinite iterator.
pub(crate) fn any_infinite_iterables<'a>(
    iter: impl IntoIterator<Item = &'a Expr>,
    semantic: &SemanticModel,
) -> bool {
    iter.into_iter()
        .any(|arg| is_infinite_iterable(arg, semantic))
}
