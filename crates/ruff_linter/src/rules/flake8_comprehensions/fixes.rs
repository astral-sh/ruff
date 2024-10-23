use std::iter;

use anyhow::{bail, Result};
use itertools::Itertools;
use libcst_native::{
    Arg, AssignEqual, AssignTargetExpression, Call, Comma, Comment, CompFor, Dict, DictComp,
    DictElement, Element, EmptyLine, Expression, GeneratorExp, LeftCurlyBrace, LeftParen,
    LeftSquareBracket, ListComp, Name, ParenthesizableWhitespace, ParenthesizedNode,
    ParenthesizedWhitespace, RightCurlyBrace, RightParen, RightSquareBracket, SetComp,
    SimpleString, SimpleWhitespace, TrailingWhitespace, Tuple,
};

use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Stylist;
use ruff_python_semantic::SemanticModel;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::cst::helpers::{negate, space};
use crate::fix::codemods::CodegenStylist;
use crate::fix::edits::pad;
use crate::rules::flake8_comprehensions::rules::ObjectType;
use crate::{
    checkers::ast::Checker,
    cst::matchers::{
        match_arg, match_call, match_call_mut, match_expression, match_generator_exp, match_lambda,
        match_list_comp, match_tuple,
    },
};

/// (C402) Convert `dict((x, x) for x in range(3))` to `{x: x for x in
/// range(3)}`.
pub(crate) fn fix_unnecessary_generator_dict(expr: &Expr, checker: &Checker) -> Result<Edit> {
    let locator = checker.locator();
    let stylist = checker.stylist();

    let module_text = locator.slice(expr);
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

    // Insert whitespace before the `for`, since we're removing parentheses, as in:
    // ```python
    // dict((x, x)for x in range(3))
    // ```
    let mut for_in = generator_exp.for_in.clone();
    if for_in.whitespace_before == ParenthesizableWhitespace::default() {
        for_in.whitespace_before =
            ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" "));
    }

    tree = Expression::DictComp(Box::new(DictComp {
        key: Box::new(key.clone()),
        value: Box::new(value.clone()),
        for_in,
        lbrace: LeftCurlyBrace {
            whitespace_after: call.whitespace_before_args.clone(),
        },
        rbrace: RightCurlyBrace {
            whitespace_before: arg.whitespace_after_arg.clone(),
        },
        lpar: vec![],
        rpar: vec![],
        whitespace_before_colon: ParenthesizableWhitespace::default(),
        whitespace_after_colon: space(),
    }));

    Ok(Edit::range_replacement(
        pad_expression(
            tree.codegen_stylist(stylist),
            expr.range(),
            checker.locator(),
            checker.semantic(),
        ),
        expr.range(),
    ))
}

/// (C404) Convert `dict([(i, i) for i in range(3)])` to `{i: i for i in
/// range(3)}`.
pub(crate) fn fix_unnecessary_list_comprehension_dict(
    expr: &Expr,
    checker: &Checker,
) -> Result<Edit> {
    let locator = checker.locator();
    let stylist = checker.stylist();

    let module_text = locator.slice(expr);
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;
    let arg = match_arg(call)?;

    let list_comp = match_list_comp(&arg.value)?;

    let tuple = match_tuple(&list_comp.elt)?;

    let [Element::Simple { value: key, .. }, Element::Simple { value, .. }] = &tuple.elements[..]
    else {
        bail!("Expected tuple with two elements");
    };

    // Insert whitespace before the `for`, since we're removing parentheses, as in:
    // ```python
    // dict((x, x)for x in range(3))
    // ```
    let mut for_in = list_comp.for_in.clone();
    if for_in.whitespace_before == ParenthesizableWhitespace::default() {
        for_in.whitespace_before =
            ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(" "));
    }

    tree = Expression::DictComp(Box::new(DictComp {
        key: Box::new(key.clone()),
        value: Box::new(value.clone()),
        for_in,
        whitespace_before_colon: ParenthesizableWhitespace::default(),
        whitespace_after_colon: space(),
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
        pad_expression(
            tree.codegen_stylist(stylist),
            expr.range(),
            checker.locator(),
            checker.semantic(),
        ),
        expr.range(),
    ))
}

/// (C406) Convert `dict([(1, 2)])` to `{1: 2}`.
pub(crate) fn fix_unnecessary_literal_dict(expr: &Expr, checker: &Checker) -> Result<Edit> {
    let locator = checker.locator();
    let stylist = checker.stylist();

    // Expr(Call(List|Tuple)))) -> Expr(Dict)))
    let module_text = locator.slice(expr);
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
                if let Some(Element::Simple { value: key, .. }) = tuple.elements.first() {
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
        pad_expression(
            tree.codegen_stylist(stylist),
            expr.range(),
            checker.locator(),
            checker.semantic(),
        ),
        expr.range(),
    ))
}

/// (C408) Convert `dict(a=1, b=2)` to `{"a": 1, "b": 2}`.
pub(crate) fn fix_unnecessary_collection_call(
    expr: &ast::ExprCall,
    checker: &Checker,
) -> Result<Edit> {
    let locator = checker.locator();
    let stylist = checker.stylist();

    // Expr(Call("dict")))) -> Expr(Dict)
    let module_text = locator.slice(expr);
    let mut tree = match_expression(module_text)?;
    let call = match_call(&tree)?;

    // Arena allocator used to create formatted strings of sufficient lifetime,
    // below.
    let mut arena: Vec<String> = vec![];

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
            whitespace_after_colon: ParenthesizableWhitespace::SimpleWhitespace(SimpleWhitespace(
                " ",
            )),
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

    Ok(Edit::range_replacement(
        pad_expression(
            tree.codegen_stylist(stylist),
            expr.range(),
            checker.locator(),
            checker.semantic(),
        ),
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
pub(crate) fn pad_expression(
    content: String,
    range: TextRange,
    locator: &Locator,
    semantic: &SemanticModel,
) -> String {
    if !semantic.in_f_string() {
        return content;
    }

    // If the expression is immediately preceded by an opening brace, then
    // we need to add a space before the expression.
    let prefix = locator.up_to(range.start());
    let left_pad = matches!(prefix.chars().next_back(), Some('{'));

    // If the expression is immediately preceded by an opening brace, then
    // we need to add a space before the expression.
    let suffix = locator.after(range.end());
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

/// Like [`pad_expression`], but only pads the start of the expression.
pub(crate) fn pad_start(
    content: &str,
    range: TextRange,
    locator: &Locator,
    semantic: &SemanticModel,
) -> String {
    if !semantic.in_f_string() {
        return content.into();
    }

    // If the expression is immediately preceded by an opening brace, then
    // we need to add a space before the expression.
    let prefix = locator.up_to(range.start());
    if matches!(prefix.chars().next_back(), Some('{')) {
        format!(" {content}")
    } else {
        content.into()
    }
}

/// Like [`pad_expression`], but only pads the end of the expression.
pub(crate) fn pad_end(
    content: &str,
    range: TextRange,
    locator: &Locator,
    semantic: &SemanticModel,
) -> String {
    if !semantic.in_f_string() {
        return content.into();
    }

    // If the expression is immediately preceded by an opening brace, then
    // we need to add a space before the expression.
    let suffix = locator.after(range.end());
    if matches!(suffix.chars().next(), Some('}')) {
        format!("{content} ")
    } else {
        content.into()
    }
}

/// (C411) Convert `list([i * i for i in x])` to `[i * i for i in x]`.
pub(crate) fn fix_unnecessary_list_call(
    expr: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    // Expr(Call(List|Tuple)))) -> Expr(List|Tuple)))
    let module_text = locator.slice(expr);
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
    expr: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
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
            // If the `reverse` argument is used...
            let args = if inner_call.args.iter().any(|arg| {
                matches!(
                    arg.keyword,
                    Some(Name {
                        value: "reverse",
                        ..
                    })
                )
            }) {
                // Negate the `reverse` argument.
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
                            arg.value = negate(&arg.value);
                        }
                        arg
                    })
                    .collect_vec()
            } else {
                let mut args = inner_call.args.clone();

                // If necessary, parenthesize a generator expression, as a generator expression must
                // be parenthesized if it's not a solitary argument. For example, given:
                // ```python
                // reversed(sorted(i for i in range(42)))
                // ```
                // Rewrite as:
                // ```python
                // sorted((i for i in range(42)), reverse=True)
                // ```
                if let [arg] = args.as_mut_slice() {
                    if matches!(arg.value, Expression::GeneratorExp(_)) {
                        if arg.value.lpar().is_empty() && arg.value.rpar().is_empty() {
                            arg.value = arg
                                .value
                                .clone()
                                .with_parens(LeftParen::default(), RightParen::default());
                        }
                    }
                }

                // Add the `reverse=True` argument.
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
    expr: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_expression(module_text)?;
    let outer_call = match_call_mut(&mut tree)?;

    outer_call.args = match outer_call.args.split_first() {
        Some((first, rest)) => {
            let inner_call = match_call(&first.value)?;
            if let Some(arg) = inner_call
                .args
                .iter()
                .find(|argument| argument.keyword.is_none())
            {
                let mut arg = arg.clone();
                arg.comma.clone_from(&first.comma);
                arg.whitespace_after_arg = first.whitespace_after_arg.clone();
                iter::once(arg)
                    .chain(rest.iter().cloned())
                    .collect::<Vec<_>>()
            } else {
                rest.to_vec()
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
    expr: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
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
        pad(tree.codegen_stylist(stylist), expr.range(), locator),
        expr.range(),
    ))
}

/// (C417) Convert `map(lambda x: x * 2, bar)` to `(x * 2 for x in bar)`.
pub(crate) fn fix_unnecessary_map(
    expr: &Expr,
    parent: Option<&Expr>,
    object_type: ObjectType,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Edit> {
    let module_text = locator.slice(expr);
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;

    let (lambda, iter) = match call.args.as_slice() {
        [call] => {
            let call = match_call(&call.value)?;
            let [lambda, iter] = call.args.as_slice() else {
                bail!("Expected two arguments");
            };
            let lambda = match_lambda(&lambda.value)?;
            let iter = &iter.value;
            (lambda, iter)
        }
        [lambda, iter] => {
            let lambda = match_lambda(&lambda.value)?;
            let iter = &iter.value;
            (lambda, iter)
        }
        _ => bail!("Expected a call or lambda"),
    };

    // Format the lambda target.
    let target = match lambda.params.params.as_slice() {
        // Ex) `lambda: x`
        [] => AssignTargetExpression::Name(Box::new(Name {
            value: "_",
            lpar: vec![],
            rpar: vec![],
        })),
        // Ex) `lambda x: y`
        [param] => AssignTargetExpression::Name(Box::new(param.name.clone())),
        // Ex) `lambda x, y: z`
        params => AssignTargetExpression::Tuple(Box::new(Tuple {
            elements: params
                .iter()
                .map(|param| Element::Simple {
                    value: Expression::Name(Box::new(param.name.clone())),
                    comma: None,
                })
                .collect(),
            lpar: vec![],
            rpar: vec![],
        })),
    };

    // Parenthesize the iterator, if necessary, as in:
    // ```python
    // map(lambda x: x, y if y else z)
    // ```
    let iter = iter.clone();
    let iter = if iter.lpar().is_empty()
        && iter.rpar().is_empty()
        && matches!(iter, Expression::IfExp(_) | Expression::Lambda(_))
    {
        iter.with_parens(LeftParen::default(), RightParen::default())
    } else {
        iter
    };

    let compfor = Box::new(CompFor {
        target,
        iter,
        ifs: vec![],
        inner_for_in: None,
        asynchronous: None,
        whitespace_before: space(),
        whitespace_after_for: space(),
        whitespace_before_in: space(),
        whitespace_after_in: space(),
    });

    match object_type {
        ObjectType::Generator => {
            tree = Expression::GeneratorExp(Box::new(GeneratorExp {
                elt: lambda.body.clone(),
                for_in: compfor,
                lpar: vec![LeftParen::default()],
                rpar: vec![RightParen::default()],
            }));
        }
        ObjectType::List => {
            tree = Expression::ListComp(Box::new(ListComp {
                elt: lambda.body.clone(),
                for_in: compfor,
                lbracket: LeftSquareBracket::default(),
                rbracket: RightSquareBracket::default(),
                lpar: vec![],
                rpar: vec![],
            }));
        }
        ObjectType::Set => {
            tree = Expression::SetComp(Box::new(SetComp {
                elt: lambda.body.clone(),
                for_in: compfor,
                lpar: vec![],
                rpar: vec![],
                lbrace: LeftCurlyBrace::default(),
                rbrace: RightCurlyBrace::default(),
            }));
        }
        ObjectType::Dict => {
            let elements = match lambda.body.as_ref() {
                Expression::Tuple(tuple) => &tuple.elements,
                Expression::List(list) => &list.elements,
                _ => {
                    bail!("Expected tuple or list for dictionary comprehension")
                }
            };
            let [key, value] = elements.as_slice() else {
                bail!("Expected container to include two elements");
            };
            let Element::Simple { value: key, .. } = key else {
                bail!("Expected container to use a key as the first element");
            };
            let Element::Simple { value, .. } = value else {
                bail!("Expected container to use a value as the second element");
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
    }

    let mut content = tree.codegen_stylist(stylist);

    // If the expression is embedded in an f-string, surround it with spaces to avoid
    // syntax errors.
    if matches!(object_type, ObjectType::Set | ObjectType::Dict) {
        if parent.is_some_and(Expr::is_f_string_expr) {
            content = format!(" {content} ");
        }
    }

    Ok(Edit::range_replacement(content, expr.range()))
}

/// (C419) Convert `[i for i in a]` into `i for i in a`
pub(crate) fn fix_unnecessary_comprehension_in_call(
    expr: &Expr,
    locator: &Locator,
    stylist: &Stylist,
) -> Result<Fix> {
    // Expr(ListComp) -> Expr(GeneratorExp)
    let module_text = locator.slice(expr);
    let mut tree = match_expression(module_text)?;
    let call = match_call_mut(&mut tree)?;

    let (whitespace_after, whitespace_before, elt, for_in, lpar, rpar) = match &call.args[0].value {
        Expression::ListComp(list_comp) => (
            &list_comp.lbracket.whitespace_after,
            &list_comp.rbracket.whitespace_before,
            &list_comp.elt,
            &list_comp.for_in,
            &list_comp.lpar,
            &list_comp.rpar,
        ),
        Expression::SetComp(set_comp) => (
            &set_comp.lbrace.whitespace_after,
            &set_comp.rbrace.whitespace_before,
            &set_comp.elt,
            &set_comp.for_in,
            &set_comp.lpar,
            &set_comp.rpar,
        ),
        _ => {
            bail!("Expected Expression::ListComp | Expression::SetComp");
        }
    };

    let mut new_empty_lines = vec![];

    if let ParenthesizableWhitespace::ParenthesizedWhitespace(ParenthesizedWhitespace {
        first_line,
        empty_lines,
        ..
    }) = &whitespace_after
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
        }) = &whitespace_before
        {
            Some(format!("{}{}", whitespace.0, comment.0))
        } else {
            None
        };

    call.args[0].value = Expression::GeneratorExp(Box::new(GeneratorExp {
        elt: elt.clone(),
        for_in: for_in.clone(),
        lpar: lpar.clone(),
        rpar: rpar.clone(),
    }));

    let whitespace_after_arg = match &call.args[0].comma {
        Some(comma) => {
            let whitespace_after_comma = comma.whitespace_after.clone();
            call.args[0].comma = Some(Comma {
                whitespace_after: ParenthesizableWhitespace::default(),
                ..comma.clone()
            });
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

    Ok(Fix::unsafe_edit(Edit::range_replacement(
        tree.codegen_stylist(stylist),
        expr.range(),
    )))
}
