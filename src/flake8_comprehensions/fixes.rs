use anyhow::Result;
use libcst_native::{
    Arg, Codegen, Dict, Expression, LeftCurlyBrace, LeftSquareBracket, List, ListComp,
    RightCurlyBrace, RightSquareBracket, Set, SetComp, SmallStatement, Statement, Tuple,
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
    let call = if let Expression::Call(call) = &mut body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let arg = if let Some(Arg { value, .. }) = call.args.first_mut() {
        value
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
            whitespace_before: call.whitespace_after_func.clone(),
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
    let call = if let Expression::Call(call) = &mut body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let arg = if let Some(Arg { value, .. }) = call.args.first_mut() {
        value
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
            whitespace_before: call.whitespace_after_func.clone(),
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
    let call = if let Expression::Call(call) = &mut body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let arg = if let Some(Arg { value, .. }) = call.args.first_mut() {
        value
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
            whitespace_before: call.whitespace_after_func.clone(),
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
    let arg = if let Some(Arg { value, .. }) = call.args.first_mut() {
        value
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
                whitespace_before: call.whitespace_after_func.clone(),
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
    let call = if let Expression::Call(call) = &mut body.value {
        call
    } else {
        return Err(anyhow::anyhow!("Expected node to be: Expression::Call."));
    };
    let arg = if let Some(Arg { value, .. }) = call.args.first_mut() {
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
