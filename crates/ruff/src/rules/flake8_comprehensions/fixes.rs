use anyhow::{bail, Result};
use itertools::Itertools;
use libcst_native::{
    Arg, AssignEqual, AssignTargetExpression, Call, Codegen, CodegenState, CompFor, Dict, DictComp,
    DictElement, Element, Expr, Expression, GeneratorExp, LeftCurlyBrace, LeftParen,
    LeftSquareBracket, List, ListComp, Name, ParenthesizableWhitespace, RightCurlyBrace,
    RightParen, RightSquareBracket, Set, SetComp, SimpleString, SimpleWhitespace, Tuple,
};

use ruff_diagnostics::Edit;
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::cst::matchers::{match_expr, match_module};

fn match_call<'a, 'b>(expr: &'a mut Expr<'b>) -> Result<&'a mut Call<'b>> {
    if let Expression::Call(call) = &mut expr.value {
        Ok(call)
    } else {
        bail!("Expected Expression::Call")
    }
}

fn match_arg<'a, 'b>(call: &'a Call<'b>) -> Result<&'a Arg<'b>> {
    if let Some(arg) = call.args.first() {
        Ok(arg)
    } else {
        bail!("Expected Arg")
    }
}

/// (C400) Convert `list(x for x in y)` to `[x for x in y]`.
pub fn fix_unnecessary_generator_list(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(Call(GeneratorExp)))) -> Expr(ListComp)))
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let Expression::GeneratorExp(generator_exp) = &arg.value else {
        bail!(
            "Expected Expression::GeneratorExp"
        );
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

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C401) Convert `set(x for x in y)` to `{x for x in y}`.
pub fn fix_unnecessary_generator_set(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
    parent: Option<&rustpython_parser::ast::Expr>,
) -> Result<Edit> {
    // Expr(Call(GeneratorExp)))) -> Expr(SetComp)))
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let Expression::GeneratorExp(generator_exp) = &arg.value else {
        bail!(
            "Expected Expression::GeneratorExp"
        );
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

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    let mut content = state.to_string();

    // If the expression is embedded in an f-string, surround it with spaces to avoid
    // syntax errors.
    if let Some(parent_element) = parent {
        if let &rustpython_parser::ast::ExprKind::FormattedValue { .. } = &parent_element.node {
            content = format!(" {content} ");
        }
    }

    Ok(Edit::replacement(
        content,
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C402) Convert `dict((x, x) for x in range(3))` to `{x: x for x in
/// range(3)}`.
pub fn fix_unnecessary_generator_dict(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
    parent: Option<&rustpython_parser::ast::Expr>,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    // Extract the (k, v) from `(k, v) for ...`.
    let Expression::GeneratorExp(generator_exp) = &arg.value else {
        bail!(
            "Expected Expression::GeneratorExp"
        );
    };
    let Expression::Tuple(tuple) = &generator_exp.elt.as_ref() else {
        bail!("Expected Expression::Tuple");
    };
    let Some(Element::Simple { value: key, .. }) = &tuple.elements.get(0) else {
        bail!(
            "Expected tuple to contain a key as the first element"
        );
    };
    let Some(Element::Simple { value, .. }) = &tuple.elements.get(1) else {
        bail!(
            "Expected tuple to contain a key as the second element"
        );
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
        lpar: vec![],
        rpar: vec![],
        whitespace_before_colon: ParenthesizableWhitespace::default(),
        whitespace_after_colon: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
    }));

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    let mut content = state.to_string();

    // If the expression is embedded in an f-string, surround it with spaces to avoid
    // syntax errors.
    if let Some(parent_element) = parent {
        if let &rustpython_parser::ast::ExprKind::FormattedValue { .. } = &parent_element.node {
            content = format!(" {content} ");
        }
    }

    Ok(Edit::replacement(
        content,
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C403) Convert `set([x for x in y])` to `{x for x in y}`.
pub fn fix_unnecessary_list_comprehension_set(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(Call(ListComp)))) ->
    // Expr(SetComp)))
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let Expression::ListComp(list_comp) = &arg.value else {
        bail!("Expected Expression::ListComp");
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

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C404) Convert `dict([(i, i) for i in range(3)])` to `{i: i for i in
/// range(3)}`.
pub fn fix_unnecessary_list_comprehension_dict(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let Expression::ListComp(list_comp) = &arg.value else {
        bail!("Expected Expression::ListComp")
    };

    let Expression::Tuple(tuple) = &*list_comp.elt else {
        bail!("Expected Expression::Tuple")
    };

    let [Element::Simple {
            value: key,
            comma: Some(comma),
        }, Element::Simple { value, .. }] = &tuple.elements[..] else { bail!("Expected tuple with two elements"); };

    body.value = Expression::DictComp(Box::new(DictComp {
        key: Box::new(key.clone()),
        value: Box::new(value.clone()),
        for_in: list_comp.for_in.clone(),
        whitespace_before_colon: comma.whitespace_before.clone(),
        whitespace_after_colon: comma.whitespace_after.clone(),
        lbrace: LeftCurlyBrace {
            whitespace_after: call.whitespace_before_args.clone(),
        },
        rbrace: RightCurlyBrace {
            whitespace_before: arg.whitespace_after_arg.clone(),
        },
        lpar: list_comp.lpar.clone(),
        rpar: list_comp.rpar.clone(),
    }));

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// Drop a trailing comma from a list of tuple elements.
fn drop_trailing_comma<'a>(
    tuple: &Tuple<'a>,
) -> Result<(
    Vec<Element<'a>>,
    ParenthesizableWhitespace<'a>,
    ParenthesizableWhitespace<'a>,
)> {
    let whitespace_after = tuple
        .lpar
        .first()
        .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses"))?
        .whitespace_after
        .clone();
    let whitespace_before = tuple
        .rpar
        .first()
        .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses"))?
        .whitespace_before
        .clone();

    let mut elements = tuple.elements.clone();
    if elements.len() == 1 {
        if let Some(Element::Simple {
            value,
            comma: Some(..),
            ..
        }) = elements.last()
        {
            if whitespace_before == ParenthesizableWhitespace::default()
                && whitespace_after == ParenthesizableWhitespace::default()
            {
                elements[0] = Element::Simple {
                    value: value.clone(),
                    comma: None,
                };
            }
        }
    }

    Ok((elements, whitespace_after, whitespace_before))
}

/// (C405) Convert `set((1, 2))` to `{1, 2}`.
pub fn fix_unnecessary_literal_set(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(Call(List|Tuple)))) -> Expr(Set)))
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let mut call = match_call(body)?;
    let arg = match_arg(call)?;

    let (elements, whitespace_after, whitespace_before) = match &arg.value {
        Expression::Tuple(inner) => drop_trailing_comma(inner)?,
        Expression::List(inner) => (
            inner.elements.clone(),
            inner.lbracket.whitespace_after.clone(),
            inner.rbracket.whitespace_before.clone(),
        ),
        _ => {
            bail!("Expected Expression::Tuple | Expression::List");
        }
    };

    if elements.is_empty() {
        call.args = vec![];
    } else {
        body.value = Expression::Set(Box::new(Set {
            elements,
            lbrace: LeftCurlyBrace { whitespace_after },
            rbrace: RightCurlyBrace { whitespace_before },
            lpar: vec![],
            rpar: vec![],
        }));
    }

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C406) Convert `dict([(1, 2)])` to `{1: 2}`.
pub fn fix_unnecessary_literal_dict(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(Call(List|Tuple)))) -> Expr(Dict)))
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let elements = match &arg.value {
        Expression::Tuple(inner) => &inner.elements,
        Expression::List(inner) => &inner.elements,
        _ => {
            bail!("Expected Expression::Tuple | Expression::List");
        }
    };

    let elements: Vec<DictElement> = elements
        .iter()
        .map(|element| {
            if let Element::Simple {
                value: Expression::Tuple(tuple),
                comma,
            } = element
            {
                if let Some(Element::Simple { value: key, .. }) = tuple.elements.get(0) {
                    if let Some(Element::Simple { value, .. }) = tuple.elements.get(1) {
                        return Ok(DictElement::Simple {
                            key: key.clone(),
                            value: value.clone(),
                            comma: comma.clone(),
                            whitespace_before_colon: ParenthesizableWhitespace::default(),
                            whitespace_after_colon: ParenthesizableWhitespace::SimpleWhitespace(
                                SimpleWhitespace(" "),
                            ),
                        });
                    }
                }
            }
            bail!("Expected each argument to be a tuple of length two")
        })
        .collect::<Result<Vec<DictElement>>>()?;

    body.value = Expression::Dict(Box::new(Dict {
        elements,
        lbrace: LeftCurlyBrace {
            whitespace_after: call.whitespace_before_args.clone(),
        },
        rbrace: RightCurlyBrace {
            whitespace_before: arg.whitespace_after_arg.clone(),
        },
        lpar: vec![],
        rpar: vec![],
    }));

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C408)
pub fn fix_unnecessary_collection_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(Call("list" | "tuple" | "dict")))) -> Expr(List|Tuple|Dict)
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let Expression::Name(name) = &call.func.as_ref() else {
        bail!("Expected Expression::Name");
    };

    // Arena allocator used to create formatted strings of sufficient lifetime,
    // below.
    let mut arena: Vec<String> = vec![];

    match name.value {
        "tuple" => {
            body.value = Expression::Tuple(Box::new(Tuple {
                elements: vec![],
                lpar: vec![LeftParen::default()],
                rpar: vec![RightParen::default()],
            }));
        }
        "list" => {
            body.value = Expression::List(Box::new(List {
                elements: vec![],
                lbracket: LeftSquareBracket::default(),
                rbracket: RightSquareBracket::default(),
                lpar: vec![],
                rpar: vec![],
            }));
        }
        "dict" => {
            if call.args.is_empty() {
                body.value = Expression::Dict(Box::new(Dict {
                    elements: vec![],
                    lbrace: LeftCurlyBrace::default(),
                    rbrace: RightCurlyBrace::default(),
                    lpar: vec![],
                    rpar: vec![],
                }));
            } else {
                // Quote each argument.
                for arg in &call.args {
                    let quoted = format!(
                        "{}{}{}",
                        stylist.quote(),
                        arg.keyword
                            .as_ref()
                            .expect("Expected dictionary argument to be kwarg")
                            .value,
                        stylist.quote(),
                    );
                    arena.push(quoted);
                }

                let elements = call
                    .args
                    .iter()
                    .enumerate()
                    .map(|(i, arg)| DictElement::Simple {
                        key: Expression::SimpleString(Box::new(SimpleString {
                            value: &arena[i],
                            lpar: vec![],
                            rpar: vec![],
                        })),
                        value: arg.value.clone(),
                        comma: arg.comma.clone(),
                        whitespace_before_colon: ParenthesizableWhitespace::default(),
                        whitespace_after_colon: ParenthesizableWhitespace::SimpleWhitespace(
                            SimpleWhitespace(" "),
                        ),
                    })
                    .collect();

                body.value = Expression::Dict(Box::new(Dict {
                    elements,
                    lbrace: LeftCurlyBrace {
                        whitespace_after: call.whitespace_before_args.clone(),
                    },
                    rbrace: RightCurlyBrace {
                        whitespace_before: call
                            .args
                            .last()
                            .expect("Arguments should be non-empty")
                            .whitespace_after_arg
                            .clone(),
                    },
                    lpar: vec![],
                    rpar: vec![],
                }));
            }
        }
        _ => {
            bail!("Expected function name to be one of: 'tuple', 'list', 'dict'");
        }
    };

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C409) Convert `tuple([1, 2])` to `tuple(1, 2)`
pub fn fix_unnecessary_literal_within_tuple_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;
    let (elements, whitespace_after, whitespace_before) = match &arg.value {
        Expression::Tuple(inner) => (
            &inner.elements,
            &inner
                .lpar
                .first()
                .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses"))?
                .whitespace_after,
            &inner
                .rpar
                .first()
                .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses"))?
                .whitespace_before,
        ),
        Expression::List(inner) => (
            &inner.elements,
            &inner.lbracket.whitespace_after,
            &inner.rbracket.whitespace_before,
        ),
        _ => {
            bail!("Expected Expression::Tuple | Expression::List");
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

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C410) Convert `list([1, 2])` to `[1, 2]`
pub fn fix_unnecessary_literal_within_list_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;
    let (elements, whitespace_after, whitespace_before) = match &arg.value {
        Expression::Tuple(inner) => (
            &inner.elements,
            &inner
                .lpar
                .first()
                .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses"))?
                .whitespace_after,
            &inner
                .rpar
                .first()
                .ok_or_else(|| anyhow::anyhow!("Expected at least one set of parentheses"))?
                .whitespace_before,
        ),
        Expression::List(inner) => (
            &inner.elements,
            &inner.lbracket.whitespace_after,
            &inner.rbracket.whitespace_before,
        ),
        _ => {
            bail!("Expected Expression::Tuple | Expression::List");
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
        lpar: vec![],
        rpar: vec![],
    }));

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C411) Convert `list([i * i for i in x])` to `[i * i for i in x]`.
pub fn fix_unnecessary_list_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(Call(List|Tuple)))) -> Expr(List|Tuple)))
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    body.value = arg.value.clone();

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C413) Convert `list(sorted([2, 3, 1]))` to `sorted([2, 3, 1])`.
/// (C413) Convert `reversed(sorted([2, 3, 1]))` to `sorted([2, 3, 1],
/// reverse=True)`.
pub fn fix_unnecessary_call_around_sorted(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let outer_call = match_call(body)?;
    let inner_call = match &outer_call.args[..] {
        [arg] => {
            if let Expression::Call(call) = &arg.value {
                call
            } else {
                bail!("Expected Expression::Call ");
            }
        }
        _ => {
            bail!("Expected one argument in outer function call");
        }
    };

    if let Expression::Name(outer_name) = &*outer_call.func {
        if outer_name.value == "list" {
            body.value = Expression::Call(inner_call.clone());
        } else {
            // If the `reverse` argument is used
            let args = if inner_call.args.iter().any(|arg| {
                matches!(
                    arg.keyword,
                    Some(Name {
                        value: "reverse",
                        ..
                    })
                )
            }) {
                // Negate the `reverse` argument
                inner_call
                    .args
                    .clone()
                    .into_iter()
                    .map(|mut arg| {
                        if matches!(
                            arg.keyword,
                            Some(Name {
                                value: "reverse",
                                ..
                            })
                        ) {
                            if let Expression::Name(ref val) = arg.value {
                                if val.value == "True" {
                                    // TODO: even better would be to drop the argument, as False is the default
                                    arg.value = Expression::Name(Box::new(Name {
                                        value: "False",
                                        lpar: vec![],
                                        rpar: vec![],
                                    }));
                                    arg
                                } else if val.value == "False" {
                                    arg.value = Expression::Name(Box::new(Name {
                                        value: "True",
                                        lpar: vec![],
                                        rpar: vec![],
                                    }));
                                    arg
                                } else {
                                    arg
                                }
                            } else {
                                arg
                            }
                        } else {
                            arg
                        }
                    })
                    .collect_vec()
            } else {
                let mut args = inner_call.args.clone();
                args.push(Arg {
                    value: Expression::Name(Box::new(Name {
                        value: "True",
                        lpar: vec![],
                        rpar: vec![],
                    })),
                    keyword: Some(Name {
                        value: "reverse",
                        lpar: vec![],
                        rpar: vec![],
                    }),
                    equal: Some(AssignEqual {
                        whitespace_before: ParenthesizableWhitespace::default(),
                        whitespace_after: ParenthesizableWhitespace::default(),
                    }),
                    comma: None,
                    star: "",
                    whitespace_after_star: ParenthesizableWhitespace::default(),
                    whitespace_after_arg: ParenthesizableWhitespace::default(),
                });
                args
            };

            body.value = Expression::Call(Box::new(Call {
                func: inner_call.func.clone(),
                args,
                lpar: inner_call.lpar.clone(),
                rpar: inner_call.rpar.clone(),
                whitespace_after_func: inner_call.whitespace_after_func.clone(),
                whitespace_before_args: inner_call.whitespace_before_args.clone(),
            }));
        }
    }

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C414) Convert `sorted(list(foo))` to `sorted(foo)`
pub fn fix_unnecessary_double_cast_or_process(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let body = match_expr(&mut tree)?;
    let mut outer_call = match_call(body)?;

    outer_call.args = match outer_call.args.split_first() {
        Some((first, rest)) => {
            let Expression::Call(inner_call) = &first.value else {
                bail!("Expected Expression::Call ");
            };
            if let Some(iterable) = inner_call.args.first() {
                let mut args = vec![iterable.clone()];
                args.extend_from_slice(rest);
                args
            } else {
                bail!("Expected at least one argument in inner function call");
            }
        }
        None => bail!("Expected at least one argument in outer function call"),
    };

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C416) Convert `[i for i in x]` to `list(x)`.
pub fn fix_unnecessary_comprehension(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;

    match &body.value {
        Expression::ListComp(inner) => {
            body.value = Expression::Call(Box::new(Call {
                func: Box::new(Expression::Name(Box::new(Name {
                    value: "list",
                    lpar: vec![],
                    rpar: vec![],
                }))),
                args: vec![Arg {
                    value: inner.for_in.iter.clone(),
                    keyword: None,
                    equal: None,
                    comma: None,
                    star: "",
                    whitespace_after_star: ParenthesizableWhitespace::default(),
                    whitespace_after_arg: ParenthesizableWhitespace::default(),
                }],
                lpar: vec![],
                rpar: vec![],
                whitespace_after_func: ParenthesizableWhitespace::default(),
                whitespace_before_args: ParenthesizableWhitespace::default(),
            }));
        }
        Expression::SetComp(inner) => {
            body.value = Expression::Call(Box::new(Call {
                func: Box::new(Expression::Name(Box::new(Name {
                    value: "set",
                    lpar: vec![],
                    rpar: vec![],
                }))),
                args: vec![Arg {
                    value: inner.for_in.iter.clone(),
                    keyword: None,
                    equal: None,
                    comma: None,
                    star: "",
                    whitespace_after_star: ParenthesizableWhitespace::default(),
                    whitespace_after_arg: ParenthesizableWhitespace::default(),
                }],
                lpar: vec![],
                rpar: vec![],
                whitespace_after_func: ParenthesizableWhitespace::default(),
                whitespace_before_args: ParenthesizableWhitespace::default(),
            }));
        }
        Expression::DictComp(inner) => {
            body.value = Expression::Call(Box::new(Call {
                func: Box::new(Expression::Name(Box::new(Name {
                    value: "dict",
                    lpar: vec![],
                    rpar: vec![],
                }))),
                args: vec![Arg {
                    value: inner.for_in.iter.clone(),
                    keyword: None,
                    equal: None,
                    comma: None,
                    star: "",
                    whitespace_after_star: ParenthesizableWhitespace::default(),
                    whitespace_after_arg: ParenthesizableWhitespace::default(),
                }],
                lpar: vec![],
                rpar: vec![],
                whitespace_after_func: ParenthesizableWhitespace::default(),
                whitespace_before_args: ParenthesizableWhitespace::default(),
            }));
        }
        _ => {
            bail!("Expected Expression::ListComp | Expression:SetComp | Expression:DictComp");
        }
    }

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C417) Convert `map(lambda x: x * 2, bar)` to `(x * 2 for x in bar)`.
pub fn fix_unnecessary_map(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
    parent: Option<&rustpython_parser::ast::Expr>,
    kind: &str,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    let (args, lambda_func) = match &arg.value {
        Expression::Call(outer_call) => {
            let inner_lambda = outer_call.args.first().unwrap().value.clone();
            match &inner_lambda {
                Expression::Lambda(..) => (outer_call.args.clone(), inner_lambda),
                _ => {
                    bail!("Expected a lambda function")
                }
            }
        }
        Expression::Lambda(..) => (call.args.clone(), arg.value.clone()),
        _ => {
            bail!("Expected a lambda or call")
        }
    };

    let Expression::Lambda(func_body) = &lambda_func else {
        bail!("Expected a lambda")
    };

    if args.len() == 2 {
        if func_body.params.params.iter().any(|f| f.default.is_some()) {
            bail!("Currently not supporting default values");
        }

        let mut args_str = func_body
            .params
            .params
            .iter()
            .map(|f| f.name.value)
            .join(", ");
        if args_str.is_empty() {
            args_str = "_".to_string();
        }

        let compfor = Box::new(CompFor {
            target: AssignTargetExpression::Name(Box::new(Name {
                value: args_str.as_str(),
                lpar: vec![],
                rpar: vec![],
            })),
            iter: args.last().unwrap().value.clone(),
            ifs: vec![],
            inner_for_in: None,
            asynchronous: None,
            whitespace_before: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
            whitespace_after_for: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(
                " ",
            )),
            whitespace_before_in: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(
                " ",
            )),
            whitespace_after_in: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
        });

        match kind {
            "generator" => {
                body.value = Expression::GeneratorExp(Box::new(GeneratorExp {
                    elt: func_body.body.clone(),
                    for_in: compfor,
                    lpar: vec![LeftParen::default()],
                    rpar: vec![RightParen::default()],
                }));
            }
            "list" => {
                body.value = Expression::ListComp(Box::new(ListComp {
                    elt: func_body.body.clone(),
                    for_in: compfor,
                    lbracket: LeftSquareBracket::default(),
                    rbracket: RightSquareBracket::default(),
                    lpar: vec![],
                    rpar: vec![],
                }));
            }
            "set" => {
                body.value = Expression::SetComp(Box::new(SetComp {
                    elt: func_body.body.clone(),
                    for_in: compfor,
                    lpar: vec![],
                    rpar: vec![],
                    lbrace: LeftCurlyBrace::default(),
                    rbrace: RightCurlyBrace::default(),
                }));
            }
            "dict" => {
                let (key, value) = if let Expression::Tuple(tuple) = func_body.body.as_ref() {
                    if tuple.elements.len() != 2 {
                        bail!("Expected two elements")
                    }

                    let Some(Element::Simple { value: key, .. }) = &tuple.elements.get(0) else {
                        bail!(
                            "Expected tuple to contain a key as the first element"
                        );
                    };
                    let Some(Element::Simple { value, .. }) = &tuple.elements.get(1) else {
                        bail!(
                            "Expected tuple to contain a key as the second element"
                        );
                    };

                    (key, value)
                } else {
                    bail!("Expected tuple for dict comprehension")
                };

                body.value = Expression::DictComp(Box::new(DictComp {
                    for_in: compfor,
                    lpar: vec![],
                    rpar: vec![],
                    key: Box::new(key.clone()),
                    value: Box::new(value.clone()),
                    lbrace: LeftCurlyBrace::default(),
                    rbrace: RightCurlyBrace::default(),
                    whitespace_before_colon: ParenthesizableWhitespace::default(),
                    whitespace_after_colon: ParenthesizableWhitespace::SimpleWhitespace(
                        SimpleWhitespace(" "),
                    ),
                }));
            }
            _ => {
                bail!("Expected generator, list, set or dict");
            }
        }

        let mut state = CodegenState {
            default_newline: &stylist.line_ending(),
            default_indent: stylist.indentation(),
            ..CodegenState::default()
        };
        tree.codegen(&mut state);

        let mut content = state.to_string();

        // If the expression is embedded in an f-string, surround it with spaces to avoid
        // syntax errors.
        if kind == "set" || kind == "dict" {
            if let Some(parent_element) = parent {
                if let &rustpython_parser::ast::ExprKind::FormattedValue { .. } =
                    &parent_element.node
                {
                    content = format!(" {content} ");
                }
            }
        }

        Ok(Edit::replacement(
            content,
            expr.location,
            expr.end_location.unwrap(),
        ))
    } else {
        bail!("Should have two arguments");
    }
}

/// (C418) Convert `dict({"a": 1})` to `{"a": 1}`
pub fn fix_unnecessary_literal_within_dict_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let mut body = match_expr(&mut tree)?;
    let call = match_call(body)?;
    let arg = match_arg(call)?;

    body.value = arg.value.clone();

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}

/// (C419) Convert `[i for i in a]` into `i for i in a`
pub fn fix_unnecessary_comprehension_any_all(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(ListComp) -> Expr(GeneratorExp)
    let module_text = locator.slice(expr);
    let mut tree = match_module(module_text)?;
    let body = match_expr(&mut tree)?;
    let call = match_call(body)?;

    let Expression::ListComp(list_comp) = &call.args[0].value else {
        bail!(
            "Expected Expression::ListComp"
        );
    };

    call.args[0].value = Expression::GeneratorExp(Box::new(GeneratorExp {
        elt: list_comp.elt.clone(),
        for_in: list_comp.for_in.clone(),
        lpar: list_comp.lpar.clone(),
        rpar: list_comp.rpar.clone(),
    }));

    if let Some(comma) = &call.args[0].comma {
        call.args[0].whitespace_after_arg = comma.whitespace_after.clone();
        call.args[0].comma = None;
    }

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    tree.codegen(&mut state);

    Ok(Edit::replacement(
        state.to_string(),
        expr.location,
        expr.end_location.unwrap(),
    ))
}
