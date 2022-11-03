use anyhow::Result;
use libcst_native::{
    Arg, Call, Codegen, Dict, DictComp, Element, Expr, Expression, LeftCurlyBrace, LeftParen,
    LeftSquareBracket, List, ListComp, Module, ParenthesizableWhitespace, RightCurlyBrace,
    RightParen, RightSquareBracket, Set, SetComp, SimpleWhitespace, SmallStatement, Statement,
    Tuple,
};

use crate::autofix::Fix;
use crate::cst::matchers::match_tree;
use crate::source_code_locator::SourceCodeLocator;

fn match_expr<'a, 'b>(module: &'a mut Module<'b>) -> Result<&'a mut Expr<'b>> {
    if let Some(Statement::Simple(expr)) = module.body.first_mut() {
        if let Some(SmallStatement::Expr(expr)) = expr.body.first_mut() {
            Ok(expr)
        } else {
            Err(anyhow::anyhow!(
                "Expected node to be: SmallStatement::Expr."
            ))
        }
    } else {
        Err(anyhow::anyhow!("Expected node to be: Statement::Simple."))
    }
}

fn match_call<'a, 'b>(expr: &'a mut Expr<'b>) -> Result<&'a mut Call<'b>> {
    if let Expression::Call(call) = &mut expr.value {
        Ok(call)
    } else {
        Err(anyhow::anyhow!("Expected node to be: Expression::Call."))
    }
}

fn match_arg<'a, 'b>(call: &'a Call<'b>) -> Result<&'a Arg<'b>> {
    if let Some(arg) = call.args.first() {
        Ok(arg)
    } else {
        Err(anyhow::anyhow!("Expected node to be: Arg."))
    }
}

/// (C400) Convert `list(x for x in y)` to `[x for x in y]`.
pub fn fix_unnecessary_generator_list(
    locator: &SourceCodeLocator,
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    // Expr(Call(GeneratorExp)))) -> Expr(ListComp)))
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let generator_exp = if let Expression::GeneratorExp(generator_exp) = &arg.value {
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
            whitespace_before: arg.whitespace_after_arg.clone(),
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
pub fn fix_unnecessary_generator_set(
    locator: &SourceCodeLocator,
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    // Expr(Call(GeneratorExp)))) -> Expr(SetComp)))
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let generator_exp = if let Expression::GeneratorExp(generator_exp) = &arg.value {
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
            whitespace_before: arg.whitespace_after_arg.clone(),
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
pub fn fix_unnecessary_generator_dict(
    locator: &SourceCodeLocator,
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    // Extract the (k, v) from `(k, v) for ...`.
    let generator_exp = if let Expression::GeneratorExp(generator_exp) = &arg.value {
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
            whitespace_before: arg.whitespace_after_arg.clone(),
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
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    // Expr(Call(ListComp)))) ->
    // Expr(SetComp)))
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let list_comp = if let Expression::ListComp(list_comp) = &arg.value {
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
            whitespace_before: arg.whitespace_after_arg.clone(),
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
pub fn fix_unnecessary_literal_set(
    locator: &SourceCodeLocator,
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    // Expr(Call(List|Tuple)))) -> Expr(Set)))
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let mut call = match_call(body)?;
    let arg = match_arg(call)?;

    let elements = match &arg.value {
        Expression::Tuple(inner) => &inner.elements,
        Expression::List(inner) => &inner.elements,
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
            elements: elements.clone(),
            lbrace: LeftCurlyBrace {
                whitespace_after: call.whitespace_before_args.clone(),
            },
            rbrace: RightCurlyBrace {
                whitespace_before: arg.whitespace_after_arg.clone(),
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
pub fn fix_unnecessary_collection_call(
    locator: &SourceCodeLocator,
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    // Expr(Call("list" | "tuple" | "dict")))) -> Expr(List|Tuple|Dict)))
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let name = if let Expression::Name(name) = &call.func.as_ref() {
        name
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Name."));
    };

    match name.value {
        "tuple" => {
            body.value = Expression::Tuple(Box::new(Tuple {
                elements: Default::default(),
                lpar: vec![Default::default()],
                rpar: vec![Default::default()],
            }));
        }
        "list" => {
            body.value = Expression::List(Box::new(List {
                elements: Default::default(),
                lbracket: Default::default(),
                rbracket: Default::default(),
                lpar: Default::default(),
                rpar: Default::default(),
            }));
        }
        "dict" => {
            body.value = Expression::Dict(Box::new(Dict {
                elements: Default::default(),
                lbrace: Default::default(),
                rbrace: Default::default(),
                lpar: Default::default(),
                rpar: Default::default(),
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
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;
    let (elements, whitespace_after, whitespace_before) = match &arg.value {
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
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;
    let (elements, whitespace_after, whitespace_before) = match &arg.value {
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
pub fn fix_unnecessary_list_call(
    locator: &SourceCodeLocator,
    expr: &rustpython_ast::Expr,
) -> Result<Fix> {
    // Expr(Call(List|Tuple)))) -> Expr(List|Tuple)))
    let mut tree = match_tree(locator, expr)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    body.value = arg.value.clone();

    let mut state = Default::default();
    tree.codegen(&mut state);

    Ok(Fix::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}
