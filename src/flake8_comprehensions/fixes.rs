use anyhow::Result;
use libcst_native::{
    Arg, Codegen, Dict, DictComp, Element, Expression, LeftCurlyBrace, LeftParen,
    LeftSquareBracket, List, ListComp, ParenthesizableWhitespace, RightCurlyBrace, RightParen,
    RightSquareBracket, Set, SetComp, SimpleWhitespace, SmallStatement, Statement, Tuple,
};
use rustpython_ast::Expr;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::source_code_locator::SourceCodeLocator;

/// (C400) Convert `list(x for x in y)` to `[x for x in y]`.
pub fn fix_unnecessary_generator_list(locator: &SourceCodeLocator, expr: &Expr) -> Result<Fix> {
    // Module(SimpleStatementLine(Expr(Call(GeneratorExp)))) ->
    // Module(SimpleStatementLine(Expr(ListComp)))
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let (arg, whitespace_after_arg) = if let Some(Arg {
        value,
        whitespace_after_arg,
        ..
    }) = call.args.first()
    {
        (value, whitespace_after_arg)
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Arg."));
    };
    let generator_exp = if let Expression::GeneratorExp(generator_exp) = &arg {
        generator_exp
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: Expression::GeneratorExp."
        ));
    };

    body.value = Expression::ListComp(Box::new(ListComp {
        elt: generator_exp.elt.clone(),
        for_in: generator_exp.for_in.clone(),
        lbracket: LeftSquareBracket {
            whitespace_after: call.whitespace_before_args.clone(),
        },
        rbracket: RightSquareBracket {
            whitespace_before: whitespace_after_arg.clone(),
        },
        lpar: generator_exp.lpar.clone(),
        rpar: generator_exp.rpar.clone(),
    }));

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C401) Convert `set(x for x in y)` to `{x for x in y}`.
pub fn fix_unnecessary_generator_set(locator: &SourceCodeLocator, expr: &Expr) -> Result<Fix> {
    // Module(SimpleStatementLine(Expr(Call(GeneratorExp)))) ->
    // Module(SimpleStatementLine(Expr(SetComp)))
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let (arg, whitespace_after_arg) = if let Some(Arg {
        value,
        whitespace_after_arg,
        ..
    }) = call.args.first()
    {
        (value, whitespace_after_arg)
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Arg."));
    };
    let generator_exp = if let Expression::GeneratorExp(generator_exp) = &arg {
        generator_exp
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: Expression::GeneratorExp."
        ));
    };

    body.value = Expression::SetComp(Box::new(SetComp {
        elt: generator_exp.elt.clone(),
        for_in: generator_exp.for_in.clone(),
        lbrace: LeftCurlyBrace {
            whitespace_after: call.whitespace_before_args.clone(),
        },
        rbrace: RightCurlyBrace {
            whitespace_before: whitespace_after_arg.clone(),
        },
        lpar: generator_exp.lpar.clone(),
        rpar: generator_exp.rpar.clone(),
    }));

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C402) Convert `dict((x, x) for x in range(3))` to `{x: x for x in
/// range(3)}`.
pub fn fix_unnecessary_generator_dict(locator: &SourceCodeLocator, expr: &Expr) -> Result<Fix> {
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let (arg, whitespace_after_arg) = if let Some(Arg {
        value,
        whitespace_after_arg,
        ..
    }) = call.args.first()
    {
        (value, whitespace_after_arg)
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Arg."));
    };
    let generator_exp = if let Expression::GeneratorExp(generator_exp) = &arg {
        generator_exp
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: Expression::GeneratorExp."
        ));
    };
    let tuple = if let Expression::Tuple(tuple) = &generator_exp.elt.as_ref() {
        tuple
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Tuple."));
    };
    let key = if let Some(Element::Simple { value, .. }) = &tuple.elements.get(0) {
        value
    } else {
        return Err(anyhow::anyhow!(
            "Expected tuple to contain a key as the first element."
        ));
    };
    let value = if let Some(Element::Simple { value, .. }) = &tuple.elements.get(1) {
        value
    } else {
        return Err(anyhow::anyhow!(
            "Expected tuple to contain a key as the second element."
        ));
    };

    body.value = Expression::DictComp(Box::new(DictComp {
        key: Box::new(key.clone()),
        value: Box::new(value.clone()),
        for_in: generator_exp.for_in.clone(),
        lbrace: LeftCurlyBrace {
            whitespace_after: call.whitespace_before_args.clone(),
        },
        rbrace: RightCurlyBrace {
            whitespace_before: whitespace_after_arg.clone(),
        },
        lpar: Default::default(),
        rpar: Default::default(),
        whitespace_before_colon: Default::default(),
        whitespace_after_colon: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
    }));

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C403) Convert `set([x for x in y])` to `{x for x in y}`.
pub fn fix_unnecessary_list_comprehension_set(
    locator: &SourceCodeLocator,
    expr: &Expr,
) -> Result<Fix> {
    // Module(SimpleStatementLine(Expr(Call(ListComp)))) ->
    // Module(SimpleStatementLine(Expr(SetComp)))
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let (arg, whitespace_after_arg) = if let Some(Arg {
        value,
        whitespace_after_arg,
        ..
    }) = call.args.first()
    {
        (value, whitespace_after_arg)
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Arg."));
    };
    let list_comp = if let Expression::ListComp(list_comp) = arg {
        list_comp
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: Expression::ListComp."
        ));
    };

    body.value = Expression::SetComp(Box::new(SetComp {
        elt: list_comp.elt.clone(),
        for_in: list_comp.for_in.clone(),
        lbrace: LeftCurlyBrace {
            whitespace_after: call.whitespace_before_args.clone(),
        },
        rbrace: RightCurlyBrace {
            whitespace_before: whitespace_after_arg.clone(),
        },
        lpar: list_comp.lpar.clone(),
        rpar: list_comp.rpar.clone(),
    }));

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C405) Convert `set((1, 2))` to `{1, 2}`.
pub fn fix_unnecessary_literal_set(locator: &SourceCodeLocator, expr: &Expr) -> Result<Fix> {
    // Module(SimpleStatementLine(Expr(Call(List|Tuple)))) ->
    // Module(SimpleStatementLine(Expr(Set)))
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &mut body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let (arg, whitespace_after_arg) = if let Some(Arg {
        value,
        whitespace_after_arg,
        ..
    }) = call.args.first_mut()
    {
        (value, whitespace_after_arg)
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Arg."));
    };
    let elements = match arg {
        Expression::Tuple(inner) => inner.elements.clone(),
        Expression::List(inner) => inner.elements.clone(),
        _ => {
            return Err(anyhow::anyhow!(
                "Expected node to be: Expression::Tuple | Expression::List."
            ))
        }
    };

    if elements.is_empty() {
        call.args = vec![];
    } else {
        body.value = Expression::Set(Box::new(Set {
            elements,
            lbrace: LeftCurlyBrace {
                whitespace_after: call.whitespace_before_args.clone(),
            },
            rbrace: RightCurlyBrace {
                whitespace_before: whitespace_after_arg.clone(),
            },
            lpar: Default::default(),
            rpar: Default::default(),
        }));
    }

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C408)
pub fn fix_unnecessary_collection_call(locator: &SourceCodeLocator, expr: &Expr) -> Result<Fix> {
    // Module(SimpleStatementLine(Expr(Call("list" | "tuple" | "dict")))) ->
    // Module(SimpleStatementLine(Expr(List|Tuple|Dict)))
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let name = if let Expression::Name(name) = &call.func.as_ref() {
        name
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Name."));
    };

    match name.value {
        "tuple" => {
            body.value = Expression::Tuple(Box::new(Tuple {
                elements: vec![],
                lpar: vec![Default::default()],
                rpar: vec![Default::default()],
            }));
        }
        "list" => {
            body.value = Expression::List(Box::new(List {
                elements: vec![],
                lbracket: Default::default(),
                rbracket: Default::default(),
                lpar: vec![],
                rpar: vec![],
            }));
        }
        "dict" => {
            body.value = Expression::Dict(Box::new(Dict {
                elements: vec![],
                lbrace: Default::default(),
                rbrace: Default::default(),
                lpar: vec![],
                rpar: vec![],
            }));
        }
        _ => {
            return Err(anyhow::anyhow!("Expected function name to be one of: \
                                        'tuple', 'list', 'dict'."
                .to_string()));
        }
    };

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C409) Convert `tuple([1, 2])` to `tuple(1, 2)`
pub fn fix_unnecessary_literal_within_tuple_call(
    locator: &SourceCodeLocator,
    expr: &Expr,
) -> Result<Fix> {
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let arg = if let Some(Arg { value, .. }) = call.args.first() {
        value
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Arg."));
    };
    let (elements, whitespace_after, whitespace_before) = match arg {
        Expression::Tuple(inner) => (
            &inner.elements,
            &inner
                .lpar
                .first()
                .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses."))?
                .whitespace_after,
            &inner
                .rpar
                .first()
                .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses."))?
                .whitespace_before,
        ),
        Expression::List(inner) => (
            &inner.elements,
            &inner.lbracket.whitespace_after,
            &inner.rbracket.whitespace_before,
        ),
        _ => {
            return Err(anyhow::anyhow!(
                "Expected node to be: Expression::Tuple | Expression::List."
            ))
        }
    };

    body.value = Expression::Tuple(Box::new(Tuple {
        elements: elements.clone(),
        lpar: vec![LeftParen {
            whitespace_after: whitespace_after.clone(),
        }],
        rpar: vec![RightParen {
            whitespace_before: whitespace_before.clone(),
        }],
    }));

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C410) Convert `list([1, 2])` to `[1, 2]`
pub fn fix_unnecessary_literal_within_list_call(
    locator: &SourceCodeLocator,
    expr: &Expr,
) -> Result<Fix> {
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let arg = if let Some(Arg { value, .. }) = call.args.first() {
        value
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Arg."));
    };
    let (elements, whitespace_after, whitespace_before) = match arg {
        Expression::Tuple(inner) => (
            &inner.elements,
            &inner
                .lpar
                .first()
                .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses."))?
                .whitespace_after,
            &inner
                .rpar
                .first()
                .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses."))?
                .whitespace_before,
        ),
        Expression::List(inner) => (
            &inner.elements,
            &inner.lbracket.whitespace_after,
            &inner.rbracket.whitespace_before,
        ),
        _ => {
            return Err(anyhow::anyhow!(
                "Expected node to be: Expression::Tuple | Expression::List."
            ))
        }
    };

    body.value = Expression::List(Box::new(List {
        elements: elements.clone(),
        lbracket: LeftSquareBracket {
            whitespace_after: whitespace_after.clone(),
        },
        rbracket: RightSquareBracket {
            whitespace_before: whitespace_before.clone(),
        },
        lpar: Default::default(),
        rpar: Default::default(),
    }));

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C411) Convert `list([i for i in x])` to `[i for i in x]`.
pub fn fix_unnecessary_list_call(locator: &SourceCodeLocator, expr: &Expr) -> Result<Fix> {
    // Module(SimpleStatementLine(Expr(Call(List|Tuple)))) ->
    // Module(SimpleStatementLine(Expr(List|Tuple)))
    let mut tree = match libcst_native::parse_module(
        locator.slice_source_code_range(&Range::from_located(expr)),
        None,
    ) {
        Ok(m) => m,
        Err(_) => return Err(anyhow::anyhow!("Failed to extract CST from source.")),
    };
    let body = if let Some(Statement::Simple(body)) = tree.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Statement::Simple."));
    };
    let body = if let Some(SmallStatement::Expr(body)) = body.body.first_mut() {
        body
    } else {
        return Err(anyhow::anyhow!(
            "Expected node to be: SmallStatement::Expr."
        ));
    };
    let call = if let Expression::Call(call) = &body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let arg = if let Some(Arg { value, .. }) = call.args.first() {
        value
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Arg."));
    };

    body.value = arg.clone();

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}
