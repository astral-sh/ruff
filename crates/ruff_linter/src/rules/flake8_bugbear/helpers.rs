use ruff_notebook::CellOffsets;
use ruff_python_ast::{
    ArgOrKeyword, Arguments, AstNodeId, Builder, Expr, ExprContext, ExprKind, ListId,
};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer};
use ruff_text_size::{Ranged, TextRange};

use crate::Locator;
use crate::fix::syntax;

/// Builds the single assignment or deletion target for a constant attribute name.
pub(super) fn attribute_target(
    builder: &mut Builder,
    object: Expr<'_>,
    attribute: &str,
    ctx: ExprContext,
) -> ListId<AstNodeId<ruff_python_ast::kind::Expr>> {
    let value = builder.copy_node(object);
    let target = syntax::attribute(builder, value, attribute, ctx);
    builder.list([target])
}

/// Return `true` if the statement containing the current expression is the last
/// top-level expression in the cell. This assumes that the source is a Jupyter
/// Notebook.
pub(super) fn at_last_top_level_expression_in_cell(
    semantic: &SemanticModel,
    locator: &Locator,
    cell_offsets: Option<&CellOffsets>,
) -> bool {
    if !semantic.at_top_level() {
        return false;
    }
    let current_statement_end = semantic.current_statement().end();
    cell_offsets
        .and_then(|cell_offsets| cell_offsets.containing_range(current_statement_end))
        .is_some_and(|cell_range| {
            SimpleTokenizer::new(
                locator.contents(),
                TextRange::new(current_statement_end, cell_range.end()),
            )
            .all(|token| token.kind() == SimpleTokenKind::Semi || token.kind().is_trivia())
        })
}

/// Return `true` if the [`Expr`] appears to be an infinite iterator (e.g., a call to
/// `itertools.cycle` or similar).
pub(crate) fn is_infinite_iterable(arg: Expr<'_>, semantic: &SemanticModel) -> bool {
    let ExprKind::Call(call_node) = arg.classify() else {
        return false;
    };
    let func = call_node.func();
    let arguments = call_node.arguments();

    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| match qualified_name.segments() {
            ["itertools", "cycle" | "count"] => true,
            ["itertools", "repeat"] => {
                let mut positionals = 0;
                let mut second_is_none = false;
                for item in arguments.iter_source_order() {
                    match item {
                        ArgOrKeyword::Arg(arg) => {
                            positionals += 1;
                            if positionals == 2 {
                                second_is_none = arg.is_none_literal_expr();
                            }
                        }
                        ArgOrKeyword::Keyword(keyword)
                            if keyword.arg().is_some_and(|name| name == "times")
                                && keyword.value().is_none_literal_expr() =>
                        {
                            return true;
                        }
                        ArgOrKeyword::Keyword(_) => {}
                    }
                }
                (positionals == 1 && arguments.len() == 1) || (positionals == 2 && second_is_none)
            }
            _ => false,
        })
}

/// Tests the positional arity for B905 and B912. An iterable unpacking can
/// supply any number of arguments, while an explicit `strict` suppresses both rules.
pub(crate) fn needs_strict_argument(arguments: Arguments<'_>, mut required: usize) -> bool {
    for item in arguments.iter_source_order() {
        match item {
            ArgOrKeyword::Keyword(keyword)
                if keyword.arg().is_some_and(|name| name == "strict") =>
            {
                return false;
            }
            ArgOrKeyword::Arg(arg) if required > 0 => {
                required = if arg.is_starred_expr() {
                    0
                } else {
                    required - 1
                };
            }
            _ => {}
        }
    }
    required == 0
}

/// Return `true` if any expression in the iterator appears to be an infinite iterator.
pub(crate) fn any_infinite_iterables<'a>(
    iter: impl IntoIterator<Item = Expr<'a>>,
    semantic: &SemanticModel,
) -> bool {
    iter.into_iter()
        .any(|arg| is_infinite_iterable(arg, semantic))
}
