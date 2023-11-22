use libcst_native::{
    Expression, Name, NameOrAttribute, ParenthesizableWhitespace, SimpleWhitespace, UnaryOperation,
};

fn compose_call_path_inner<'a>(expr: &'a Expression, parts: &mut Vec<&'a str>) {
    match expr {
        Expression::Call(expr) => {
            compose_call_path_inner(&expr.func, parts);
        }
        Expression::Attribute(expr) => {
            compose_call_path_inner(&expr.value, parts);
            parts.push(expr.attr.value);
        }
        Expression::Name(expr) => {
            parts.push(expr.value);
        }
        _ => {}
    }
}

pub(crate) fn compose_call_path(expr: &Expression) -> Option<String> {
    let mut segments = vec![];
    compose_call_path_inner(expr, &mut segments);
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("."))
    }
}

pub(crate) fn compose_module_path(module: &NameOrAttribute) -> String {
    match module {
        NameOrAttribute::N(name) => name.value.to_string(),
        NameOrAttribute::A(attr) => {
            let name = attr.attr.value;
            let prefix = compose_call_path(&attr.value);
            prefix.map_or_else(|| name.to_string(), |prefix| format!("{prefix}.{name}"))
        }
    }
}

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
