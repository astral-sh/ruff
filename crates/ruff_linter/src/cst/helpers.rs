use libcst_native::{
    Expression, Name, ParenthesizableWhitespace, SimpleWhitespace, UnaryOperation,
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

    Expression::UnaryOperation(Box::new(UnaryOperation {
        operator: libcst_native::UnaryOp::Not {
            whitespace_after: space(),
        },
        expression: Box::new(expression.clone()),
        lpar: vec![],
        rpar: vec![],
    }))
}
