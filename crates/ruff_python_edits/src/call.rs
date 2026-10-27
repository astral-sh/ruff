use ruff_python_ast::token::{TokenIterWithContext, Tokens, parenthesized_range};
use ruff_python_ast::{self as ast, AnyNodeRef, Expr, ExprCall, OperatorPrecedence};
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

/// Returns source code that replaces a call with one of its arguments.
///
/// Preserve the argument's optional parentheses, including any comments inside them. Otherwise, add
/// parentheses when needed for grouping, line continuation, or separation from adjacent tokens.
/// Compound expressions can remain unparenthesized in expression statements, assignment or return
/// values, and call arguments. In other contexts, including when `parent` is unknown, group them
/// conservatively.
///
/// The parent can be the enclosing expression or statement, or an `Arguments` or `Keyword` node.
///
/// Callers are responsible for determining whether removing the call is valid and whether discarding
/// its other arguments or comments affects the applicability of a fix.
pub fn unwrapped_call_argument(
    call: &ExprCall,
    argument: &Expr,
    parent: Option<AnyNodeRef>,
    tokens: &Tokens,
    source: &str,
) -> String {
    if let Some(range) = parenthesized_range(argument.into(), (&call.arguments).into(), tokens) {
        return source[range].to_string();
    }

    let argument_source = &source[argument.range()];

    // The call can separate tokens that would otherwise join after its removal: `return(int)(1)`,
    // `int(1)and other`, or `int(1).real`. In an f-string, adjacent opening braces would instead
    // escape the interpolation: `f"{cast(dict[str, int], {'a': value})}"`.
    let adjacent_before = tokens
        .before(call.start())
        .last()
        .filter(|token| token.end() == call.start());
    let adjacent_after = tokens
        .after(call.end())
        .first()
        .filter(|token| token.start() == call.end());
    let needs_boundary_parens = adjacent_before.is_some_and(|token| {
        token.kind().is_keyword() || (token.kind().is_lbrace() && argument_source.starts_with('{'))
    }) || adjacent_after.is_some_and(|token| {
        token.kind().is_keyword()
            || (token.kind().is_dot()
                && matches!(
                    argument,
                    Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: ast::Number::Int(_),
                        ..
                    })
                ))
    });

    // These positions accept a complete expression. A callee still needs grouping:
    // `consume(f if flag else g)` is valid, but calling the result needs `(f if flag else g)()`.
    let parent_allows_unparenthesized = parent.is_some_and(|parent| match parent {
        AnyNodeRef::Arguments(_) | AnyNodeRef::Keyword(_) => true,
        AnyNodeRef::ExprCall(parent) => parent.arguments.range().contains_range(call.range()),
        AnyNodeRef::StmtExpr(ast::StmtExpr { value, .. })
        | AnyNodeRef::StmtAssign(ast::StmtAssign { value, .. }) => value.range() == call.range(),
        AnyNodeRef::StmtReturn(ast::StmtReturn { value, .. })
        | AnyNodeRef::StmtAnnAssign(ast::StmtAnnAssign { value, .. }) => value
            .as_ref()
            .is_some_and(|value| value.range() == call.range()),
        _ => false,
    });

    // Newlines inside the argument's own delimiters do not need the outer call's parentheses.
    // For example, `f(\n1)` can stand alone, but `math\n.floor(1)` cannot.
    let mut needs_line_continuation = false;
    if source.contains_line_break(argument.range()) {
        let mut argument_tokens = TokenIterWithContext::new(tokens.in_range(argument.range()));
        while let Some(token) = argument_tokens.next() {
            if token.kind().is_any_newline() && !argument_tokens.in_parenthesized_context() {
                needs_line_continuation = true;
                break;
            }
        }
    }

    let needs_parens = needs_boundary_parens
        || needs_line_continuation
        || (OperatorPrecedence::from(argument) < OperatorPrecedence::CallAttribute
            && !parent_allows_unparenthesized)
        || matches!(
            argument,
            Expr::Named(_)
                | Expr::Yield(_)
                | Expr::YieldFrom(_)
                | Expr::Tuple(ast::ExprTuple {
                    parenthesized: false,
                    ..
                })
                | Expr::Generator(ast::ExprGenerator {
                    parenthesized: false,
                    ..
                })
        );

    if needs_parens {
        format!("({argument_source})")
    } else {
        argument_source.to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use ruff_python_ast::AnyNodeRef;
    use ruff_python_ast::find_node::covering_node;
    use ruff_python_parser::parse_module;
    use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

    use super::unwrapped_call_argument;

    #[test]
    fn call_unwrapping_preserves_context() -> Result<(), Box<dyn Error>> {
        for (source, expected) in [
            ("wrap(f if flag else g)()", "(f if flag else g)()"),
            (
                "[x for x in wrap(a if flag else b)]",
                "[x for x in (a if flag else b)]",
            ),
            ("f\"{wrap({})}\"", "f\"{({})}\""),
        ] {
            let parsed = parse_module(source)?;
            let start = TextSize::try_from(source.find("wrap").ok_or("missing wrap call")?)?;
            let covering = covering_node(
                parsed.syntax().into(),
                TextRange::at(start, "wrap".text_len()),
            )
            .find_first(|node| matches!(node, AnyNodeRef::ExprCall(_)))
            .map_err(|_| "missing enclosing call")?;
            let AnyNodeRef::ExprCall(call) = covering.node() else {
                return Err("expected a call expression".into());
            };
            let argument = call.arguments.args.first().ok_or("missing argument")?;
            let replacement =
                unwrapped_call_argument(call, argument, covering.parent(), parsed.tokens(), source);

            let mut fixed = source.to_string();
            fixed.replace_range(
                usize::from(call.start())..usize::from(call.end()),
                &replacement,
            );
            assert_eq!(fixed, expected, "{source}");
            parse_module(&fixed)?;
        }
        Ok(())
    }
}
