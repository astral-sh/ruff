use libcst_native::{
    Expression, LeftParen, Name, ParenthesizableWhitespace, ParenthesizedNode, RightParen,
    SimpleWhitespace, UnaryOperation,
};

/// Return a [`ParenthesizableWhitespace`] containing a single space.
pub(crate) fn space() -> ParenthesizableWhitespace<'static> {
    ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" "))
}

/// Ensure that a [`ParenthesizableWhitespace`] contains at least one space.
pub(crate) fn or_space(whitespace: ParenthesizableWhitespace) -> ParenthesizableWhitespace {
    if whitespace == ParenthesizableWhitespace::default() {
        space()
    } else {
        whitespace
    }
}

/// Negate a condition, i.e., `a` => `not a` and `not a` => `a`.
pub(crate) fn negate<'a>(expression: &Expression<'a>) -> Expression<'a> {
    if let Expression::UnaryOperation(ref expression) = expression {
        if matches!(expression.operator, libcst_native::UnaryOp::Not { .. }) {
            return *expression.expression.clone();
        }
    }

    // If the expression is `True` or `False`, return the opposite.
    if let Expression::Name(ref expression) = expression {
        match expression.value {
            "True" => {
                return Expression::Name(Box::new(Name {
                    value: "False",
                    lpar: vec![],
                    rpar: vec![],
                }));
            }
            "False" => {
                return Expression::Name(Box::new(Name {
                    value: "True",
                    lpar: vec![],
                    rpar: vec![],
                }));
            }
            _ => {}
        }
    }

    // If the expression is higher precedence than the unary `not`, we need to wrap it in
    // parentheses.
    //
    // For example: given `a and b`, we need to return `not (a and b)`, rather than `not a and b`.
    //
    // See: <https://docs.python.org/3/reference/expressions.html#operator-precedence>
    let needs_parens = matches!(
        expression,
        Expression::BooleanOperation(_)
            | Expression::IfExp(_)
            | Expression::Lambda(_)
            | Expression::NamedExpr(_)
    );
    let has_parens = !expression.lpar().is_empty() && !expression.rpar().is_empty();
    // Otherwise, wrap in a `not` operator.
    Expression::UnaryOperation(Box::new(UnaryOperation {
        operator: libcst_native::UnaryOp::Not {
            whitespace_after: space(),
        },
        expression: Box::new(if needs_parens && !has_parens {
            expression
                .clone()
                .with_parens(LeftParen::default(), RightParen::default())
        } else {
            expression.clone()
        }),
        lpar: vec![],
        rpar: vec![],
    }))
}
