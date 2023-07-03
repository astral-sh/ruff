use anyhow::{bail, Result};
use itertools::Itertools;
use libcst_native::{
    Arg, AssignEqual, AssignTargetExpression, Call, Comment, CompFor, Dict, DictComp, DictElement,
    Element, EmptyLine, Expression, GeneratorExp, LeftCurlyBrace, LeftParen, LeftSquareBracket,
    List, ListComp, Name, ParenthesizableWhitespace, ParenthesizedWhitespace, RightCurlyBrace,
    RightParen, RightSquareBracket, Set, SetComp, SimpleString, SimpleWhitespace,
    TrailingWhitespace, Tuple,
};
use ruff_text_size::TextRange;
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::autofix::codemods::CodegenStylist;
use crate::{
    checkers::ast::Checker,
    cst::matchers::{
        match_arg, match_call, match_call_mut, match_expression, match_generator_exp, match_lambda,
        match_list_comp, match_name, match_tuple,
    },
};

/// (C400) Convert `list(x for x in y)` to `[x for x in y]`.
pub(crate) fn fix_unnecessary_generator_list(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(Call(GeneratorExp)))) -> Expr(ListComp)))
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
    let arg = match_arg(call)?;

    let generator_exp = match_generator_exp(&arg.value)?;

    tree = Expression::ListComp(Box::new(ListComp {
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

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    ))
}

/// (C401) Convert `set(x for x in y)` to `{x for x in y}`.
pub(crate) fn fix_unnecessary_generator_set(
    checker: &Checker,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let locator = checker.locator;
    let stylist = checker.stylist;

    // Expr(Call(GeneratorExp)))) -> Expr(SetComp)))
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
    let arg = match_arg(call)?;

    let generator_exp = match_generator_exp(&arg.value)?;

    tree = Expression::SetComp(Box::new(SetComp {
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

    let content = tree.codegen_stylist(stylist);

    Ok(Edit::range_replacement(
        pad_expression(content, expr.range(), checker),
        expr.range(),
    ))
}

/// (C402) Convert `dict((x, x) for x in range(3))` to `{x: x for x in
/// range(3)}`.
pub(crate) fn fix_unnecessary_generator_dict(
    checker: &Checker,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let locator = checker.locator;
    let stylist = checker.stylist;

    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
    let arg = match_arg(call)?;

    // Extract the (k, v) from `(k, v) for ...`.
    let generator_exp = match_generator_exp(&arg.value)?;
    let tuple = match_tuple(&generator_exp.elt)?;
    let [Element::Simple { value: key, .. }, Element::Simple { value, .. }] = &tuple.elements[..]
    else {
        bail!("Expected tuple to contain two elements");
    };

    tree = Expression::DictComp(Box::new(DictComp {
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

    Ok(Edit::range_replacement(
        pad_expression(tree.codegen_stylist(stylist), expr.range(), checker),
        expr.range(),
    ))
}

/// (C403) Convert `set([x for x in y])` to `{x for x in y}`.
pub(crate) fn fix_unnecessary_list_comprehension_set(
    checker: &Checker,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let locator = checker.locator;
    let stylist = checker.stylist;
    // Expr(Call(ListComp)))) ->
    // Expr(SetComp)))
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
    let arg = match_arg(call)?;

    let list_comp = match_list_comp(&arg.value)?;

    tree = Expression::SetComp(Box::new(SetComp {
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

    Ok(Edit::range_replacement(
        pad_expression(tree.codegen_stylist(stylist), expr.range(), checker),
        expr.range(),
    ))
}

/// (C404) Convert `dict([(i, i) for i in range(3)])` to `{i: i for i in
/// range(3)}`.
pub(crate) fn fix_unnecessary_list_comprehension_dict(
    checker: &Checker,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let locator = checker.locator;
    let stylist = checker.stylist;

    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
    let arg = match_arg(call)?;

    let list_comp = match_list_comp(&arg.value)?;

    let tuple = match_tuple(&list_comp.elt)?;

    let [Element::Simple { value: key, .. }, Element::Simple { value, .. }] = &tuple.elements[..]
    else {
        bail!("Expected tuple with two elements");
    };

    tree = Expression::DictComp(Box::new(DictComp {
        key: Box::new(key.clone()),
        value: Box::new(value.clone()),
        for_in: list_comp.for_in.clone(),
        whitespace_before_colon: ParenthesizableWhitespace::default(),
        whitespace_after_colon: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" ")),
        lbrace: LeftCurlyBrace {
            whitespace_after: call.whitespace_before_args.clone(),
        },
        rbrace: RightCurlyBrace {
            whitespace_before: arg.whitespace_after_arg.clone(),
        },
        lpar: list_comp.lpar.clone(),
        rpar: list_comp.rpar.clone(),
    }));

    Ok(Edit::range_replacement(
        pad_expression(tree.codegen_stylist(stylist), expr.range(), checker),
        expr.range(),
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
pub(crate) fn fix_unnecessary_literal_set(
    checker: &Checker,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let locator = checker.locator;
    let stylist = checker.stylist;

    // Expr(Call(List|Tuple)))) -> Expr(Set)))
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
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
        tree = Expression::Set(Box::new(Set {
            elements,
            lbrace: LeftCurlyBrace { whitespace_after },
            rbrace: RightCurlyBrace { whitespace_before },
            lpar: vec![],
            rpar: vec![],
        }));
    }

    Ok(Edit::range_replacement(
        pad_expression(tree.codegen_stylist(stylist), expr.range(), checker),
        expr.range(),
    ))
}

/// (C406) Convert `dict([(1, 2)])` to `{1: 2}`.
pub(crate) fn fix_unnecessary_literal_dict(
    checker: &Checker,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let locator = checker.locator;
    let stylist = checker.stylist;

    // Expr(Call(List|Tuple)))) -> Expr(Dict)))
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
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

    tree = Expression::Dict(Box::new(Dict {
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

    Ok(Edit::range_replacement(
        pad_expression(tree.codegen_stylist(stylist), expr.range(), checker),
        expr.range(),
    ))
}

/// (C408)
pub(crate) fn fix_unnecessary_collection_call(
    checker: &Checker,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    enum Collection {
        Tuple,
        List,
        Dict,
    }

    let locator = checker.locator;
    let stylist = checker.stylist;

    // Expr(Call("list" | "tuple" | "dict")))) -> Expr(List|Tuple|Dict)
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call(&tree)?;
    let name = match_name(&call.func)?;
    let collection = match name.value {
        "tuple" => Collection::Tuple,
        "list" => Collection::List,
        "dict" => Collection::Dict,
        _ => bail!("Expected 'tuple', 'list', or 'dict'"),
    };

    // Arena allocator used to create formatted strings of sufficient lifetime,
    // below.
    let mut arena: Vec<String> = vec![];

    match collection {
        Collection::Tuple => {
            tree = Expression::Tuple(Box::new(Tuple {
                elements: vec![],
                lpar: vec![LeftParen::default()],
                rpar: vec![RightParen::default()],
            }));
        }
        Collection::List => {
            tree = Expression::List(Box::new(List {
                elements: vec![],
                lbracket: LeftSquareBracket::default(),
                rbracket: RightSquareBracket::default(),
                lpar: vec![],
                rpar: vec![],
            }));
        }
        Collection::Dict => {
            if call.args.is_empty() {
                tree = Expression::Dict(Box::new(Dict {
                    elements: vec![],
                    lbrace: LeftCurlyBrace::default(),
                    rbrace: RightCurlyBrace::default(),
                    lpar: vec![],
                    rpar: vec![],
                }));
            } else {
                let quote = checker.f_string_quote_style().unwrap_or(stylist.quote());

                // Quote each argument.
                for arg in &call.args {
                    let quoted = format!(
                        "{}{}{}",
                        quote,
                        arg.keyword
                            .as_ref()
                            .expect("Expected dictionary argument to be kwarg")
                            .value,
                        quote,
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

                tree = Expression::Dict(Box::new(Dict {
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
    };

    Ok(Edit::range_replacement(
        if matches!(collection, Collection::Dict) {
            pad_expression(tree.codegen_stylist(stylist), expr.range(), checker)
        } else {
            tree.codegen_stylist(stylist)
        },
        expr.range(),
    ))
}

/// Re-formats the given expression for use within a formatted string.
///
/// For example, when converting a `dict` call to a dictionary literal within
/// a formatted string, we might naively generate the following code:
///
/// ```python
/// f"{{'a': 1, 'b': 2}}"
/// ```
///
/// However, this is a syntax error under the f-string grammar. As such,
/// this method will pad the start and end of an expression as needed to
/// avoid producing invalid syntax.
fn pad_expression(content: String, range: TextRange, checker: &Checker) -> String {
    if !checker.semantic().in_f_string() {
        return content;
    }

    // If the expression is immediately preceded by an opening brace, then
    // we need to add a space before the expression.
    let prefix = checker.locator.up_to(range.start());
    let left_pad = matches!(prefix.chars().rev().next(), Some('{'));

    // If the expression is immediately preceded by an opening brace, then
    // we need to add a space before the expression.
    let suffix = checker.locator.after(range.end());
    let right_pad = matches!(suffix.chars().next(), Some('}'));

    if left_pad && right_pad {
        format!(" {content} ")
    } else if left_pad {
        format!(" {content}")
    } else if right_pad {
        format!("{content} ")
    } else {
        content
    }
}

/// (C409) Convert `tuple([1, 2])` to `tuple(1, 2)`
pub(crate) fn fix_unnecessary_literal_within_tuple_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
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

    tree = Expression::Tuple(Box::new(Tuple {
        elements: elements.clone(),
        lpar: vec![LeftParen {
            whitespace_after: whitespace_after.clone(),
        }],
        rpar: vec![RightParen {
            whitespace_before: whitespace_before.clone(),
        }],
    }));

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    ))
}

/// (C410) Convert `list([1, 2])` to `[1, 2]`
pub(crate) fn fix_unnecessary_literal_within_list_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
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

    tree = Expression::List(Box::new(List {
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

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    ))
}

/// (C411) Convert `list([i * i for i in x])` to `[i * i for i in x]`.
pub(crate) fn fix_unnecessary_list_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    // Expr(Call(List|Tuple)))) -> Expr(List|Tuple)))
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
    let arg = match_arg(call)?;

    tree = arg.value.clone();

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    ))
}

/// (C413) Convert `list(sorted([2, 3, 1]))` to `sorted([2, 3, 1])`.
/// (C413) Convert `reversed(sorted([2, 3, 1]))` to `sorted([2, 3, 1],
/// reverse=True)`.
pub(crate) fn fix_unnecessary_call_around_sorted(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let outer_call = match_call_mut(&mut tree)?;
    let inner_call = match &outer_call.args[..] {
        [arg] => match_call(&arg.value)?,
        _ => {
            bail!("Expected one argument in outer function call");
        }
    };

    if let Expression::Name(outer_name) = &*outer_call.func {
        if outer_name.value == "list" {
            tree = Expression::Call(Box::new((*inner_call).clone()));
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

            tree = Expression::Call(Box::new(Call {
                func: inner_call.func.clone(),
                args,
                lpar: inner_call.lpar.clone(),
                rpar: inner_call.rpar.clone(),
                whitespace_after_func: inner_call.whitespace_after_func.clone(),
                whitespace_before_args: inner_call.whitespace_before_args.clone(),
            }));
        }
    }

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    ))
}

/// (C414) Convert `sorted(list(foo))` to `sorted(foo)`
pub(crate) fn fix_unnecessary_double_cast_or_process(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let outer_call = match_call_mut(&mut tree)?;

    outer_call.args = match outer_call.args.split_first() {
        Some((first, rest)) => {
            let inner_call = match_call(&first.value)?;
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

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    ))
}

/// (C416) Convert `[i for i in x]` to `list(x)`.
pub(crate) fn fix_unnecessary_comprehension(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;

    match &tree {
        Expression::ListComp(inner) => {
            tree = Expression::Call(Box::new(Call {
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
            tree = Expression::Call(Box::new(Call {
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
            tree = Expression::Call(Box::new(Call {
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

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    ))
}

/// (C417) Convert `map(lambda x: x * 2, bar)` to `(x * 2 for x in bar)`.
pub(crate) fn fix_unnecessary_map(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
    parent: Option<&rustpython_parser::ast::Expr>,
    kind: &str,
) -> Result<Edit> {
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
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

    let func_body = match_lambda(&lambda_func)?;

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
                tree = Expression::GeneratorExp(Box::new(GeneratorExp {
                    elt: func_body.body.clone(),
                    for_in: compfor,
                    lpar: vec![LeftParen::default()],
                    rpar: vec![RightParen::default()],
                }));
            }
            "list" => {
                tree = Expression::ListComp(Box::new(ListComp {
                    elt: func_body.body.clone(),
                    for_in: compfor,
                    lbracket: LeftSquareBracket::default(),
                    rbracket: RightSquareBracket::default(),
                    lpar: vec![],
                    rpar: vec![],
                }));
            }
            "set" => {
                tree = Expression::SetComp(Box::new(SetComp {
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
                        bail!("Expected tuple to contain a key as the first element");
                    };
                    let Some(Element::Simple { value, .. }) = &tuple.elements.get(1) else {
                        bail!("Expected tuple to contain a key as the second element");
                    };

                    (key, value)
                } else {
                    bail!("Expected tuple for dict comprehension")
                };

                tree = Expression::DictComp(Box::new(DictComp {
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

        let mut content = tree.codegen_stylist(stylist);

        // If the expression is embedded in an f-string, surround it with spaces to avoid
        // syntax errors.
        if kind == "set" || kind == "dict" {
            if let Some(rustpython_parser::ast::Expr::FormattedValue(_)) = parent {
                content = format!(" {content} ");
            }
        }

        Ok(Edit::range_replacement(content, expr.range()))
    } else {
        bail!("Should have two arguments");
    }
}

/// (C418) Convert `dict({"a": 1})` to `{"a": 1}`
pub(crate) fn fix_unnecessary_literal_within_dict_call(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Edit> {
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
    let arg = match_arg(call)?;

    tree = arg.value.clone();

    Ok(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    ))
}

/// (C419) Convert `[i for i in a]` into `i for i in a`
pub(crate) fn fix_unnecessary_comprehension_any_all(
    locator: &Locator,
    stylist: &Stylist,
    expr: &rustpython_parser::ast::Expr,
) -> Result<Fix> {
    // Expr(ListComp) -> Expr(GeneratorExp)
    let module_text = locator.slice(expr.range());
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;

    let Expression::ListComp(list_comp) = &call.args[0].value else {
        bail!("Expected Expression::ListComp");
    };

    let mut new_empty_lines = vec![];

    if let ParenthesizableWhitespace::ParenthesizedWhitespace(ParenthesizedWhitespace {
        first_line,
        empty_lines,
        ..
    }) = &list_comp.lbracket.whitespace_after
    {
        // If there's a comment on the line after the opening bracket, we need
        // to preserve it. The way we do this is by adding a new empty line
        // with the same comment.
        //
        // Example:
        // ```python
        // any(
        //     [  # comment
        //         ...
        //     ]
        // )
        //
        // # The above code will be converted to:
        // any(
        //     # comment
        //     ...
        // )
        // ```
        if let TrailingWhitespace {
            comment: Some(comment),
            ..
        } = first_line
        {
            // The indentation should be same as that of the opening bracket,
            // but we don't have that information here. This will be addressed
            // before adding these new nodes.
            new_empty_lines.push(EmptyLine {
                comment: Some(comment.clone()),
                ..EmptyLine::default()
            });
        }
        if !empty_lines.is_empty() {
            new_empty_lines.extend(empty_lines.clone());
        }
    }

    if !new_empty_lines.is_empty() {
        call.whitespace_before_args = match &call.whitespace_before_args {
            ParenthesizableWhitespace::ParenthesizedWhitespace(ParenthesizedWhitespace {
                first_line,
                indent,
                last_line,
                ..
            }) => {
                // Add the indentation of the opening bracket to all the new
                // empty lines.
                for empty_line in &mut new_empty_lines {
                    empty_line.whitespace = last_line.clone();
                }
                ParenthesizableWhitespace::ParenthesizedWhitespace(ParenthesizedWhitespace {
                    first_line: first_line.clone(),
                    empty_lines: new_empty_lines,
                    indent: *indent,
                    last_line: last_line.clone(),
                })
            }
            // This is a rare case, but it can happen if the opening bracket
            // is on the same line as the function call.
            //
            // Example:
            // ```python
            // any([
            //         ...
            //     ]
            // )
            // ```
            ParenthesizableWhitespace::SimpleWhitespace(whitespace) => {
                for empty_line in &mut new_empty_lines {
                    empty_line.whitespace = whitespace.clone();
                }
                ParenthesizableWhitespace::ParenthesizedWhitespace(ParenthesizedWhitespace {
                    empty_lines: new_empty_lines,
                    ..ParenthesizedWhitespace::default()
                })
            }
        }
    }

    let rbracket_comment =
        if let ParenthesizableWhitespace::ParenthesizedWhitespace(ParenthesizedWhitespace {
            first_line:
                TrailingWhitespace {
                    whitespace,
                    comment: Some(comment),
                    ..
                },
            ..
        }) = &list_comp.rbracket.whitespace_before
        {
            Some(format!("{}{}", whitespace.0, comment.0))
        } else {
            None
        };

    call.args[0].value = Expression::GeneratorExp(Box::new(GeneratorExp {
        elt: list_comp.elt.clone(),
        for_in: list_comp.for_in.clone(),
        lpar: list_comp.lpar.clone(),
        rpar: list_comp.rpar.clone(),
    }));

    let whitespace_after_arg = match &call.args[0].comma {
        Some(comma) => {
            let whitespace_after_comma = comma.whitespace_after.clone();
            call.args[0].comma = None;
            whitespace_after_comma
        }
        _ => call.args[0].whitespace_after_arg.clone(),
    };

    let new_comment;
    call.args[0].whitespace_after_arg = match rbracket_comment {
        Some(existing_comment) => {
            if let ParenthesizableWhitespace::ParenthesizedWhitespace(ParenthesizedWhitespace {
                first_line:
                    TrailingWhitespace {
                        whitespace: SimpleWhitespace(whitespace),
                        comment: Some(Comment(comment)),
                        ..
                    },
                empty_lines,
                indent,
                last_line,
            }) = &whitespace_after_arg
            {
                new_comment = format!("{existing_comment}{whitespace}{comment}");
                ParenthesizableWhitespace::ParenthesizedWhitespace(ParenthesizedWhitespace {
                    first_line: TrailingWhitespace {
                        comment: Some(Comment(new_comment.as_str())),
                        ..TrailingWhitespace::default()
                    },
                    empty_lines: empty_lines.clone(),
                    indent: *indent,
                    last_line: last_line.clone(),
                })
            } else {
                whitespace_after_arg
            }
        }
        _ => whitespace_after_arg,
    };

    Ok(Fix::suggested(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    )))
}
