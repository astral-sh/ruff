use ruff_python_ast::{self as ast, Arguments, Expr};
use ruff_python_semantic::SemanticModel;

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
