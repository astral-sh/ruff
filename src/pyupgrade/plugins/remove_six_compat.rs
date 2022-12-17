use anyhow::{bail, Result};
use log::error;
use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, StmtKind};

use crate::ast::helpers::{collect_call_paths, create_expr, create_stmt, dealias_call_path};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckCode, CheckKind};
use crate::code_gen::SourceGenerator;
use crate::SourceCodeLocator;

/// Return `true` if the `Expr` is a reference to `${module}.${any}`.
fn is_module_member(call_path: &[&str], module: &str) -> bool {
    call_path
        .first()
        .map_or(false, |module_name| *module_name == module)
}

fn map_name(name: &str, expr: &Expr, patch: bool) -> Option<Check> {
    let replacement = match name {
        "text_type" => Some("str"),
        "binary_type" => Some("bytes"),
        "class_types" => Some("(type,)"),
        "string_types" => Some("(str,)"),
        "integer_types" => Some("(int,)"),
        "unichr" => Some("chr"),
        "iterbytes" => Some("iter"),
        "print_" => Some("print"),
        "exec_" => Some("exec"),
        "advance_iterator" => Some("next"),
        "next" => Some("next"),
        "range" => Some("range"),  // TODO: six.moves
        "xrange" => Some("range"), // TODO: six.moves
        "callable" => Some("callable"),
        _ => None,
    };
    if let Some(replacement) = replacement {
        let mut check = Check::new(CheckKind::RemoveSixCompat, Range::from_located(expr));
        if patch {
            check.amend(Fix::replacement(
                replacement.to_string(),
                expr.location,
                expr.end_location.unwrap(),
            ));
        }
        Some(check)
    } else {
        None
    }
}

fn replace_by_str_literal(
    arg: &Expr,
    binary: bool,
    expr: &Expr,
    patch: bool,
    locator: &SourceCodeLocator,
) -> Option<Check> {
    match &arg.node {
        ExprKind::Constant { .. } => {
            let mut check = Check::new(CheckKind::RemoveSixCompat, Range::from_located(expr));
            if patch {
                let content = format!(
                    "{}{}",
                    if binary { "b" } else { "" },
                    locator.slice_source_code_range(&Range {
                        location: arg.location,
                        end_location: arg.end_location.unwrap(),
                    })
                );
                check.amend(Fix::replacement(
                    content,
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            };
            Some(check)
        }
        _ => None,
    }
}

// `func(arg)` => `arg.attr`
fn replace_call_on_arg_by_arg_attribute(
    attr: &str,
    arg: &Expr,
    expr: &Expr,
    patch: bool,
) -> Result<Check> {
    let attribute = ExprKind::Attribute {
        value: Box::new(arg.clone()),
        attr: attr.to_string(),
        ctx: ExprContext::Load,
    };
    replace_by_expr_kind(attribute, expr, patch)
}

// `func(arg, **args)` => `arg.method(**args)`
fn replace_call_on_arg_by_arg_method_call(
    method_name: &str,
    args: &[Expr],
    expr: &Expr,
    patch: bool,
) -> Result<Option<Check>> {
    if args.is_empty() {
        bail!("Expected at least one argument");
    }
    if let ([arg], other_args) = args.split_at(1) {
        let call = ExprKind::Call {
            func: Box::new(create_expr(ExprKind::Attribute {
                value: Box::new(arg.clone()),
                attr: method_name.to_string(),
                ctx: ExprContext::Load,
            })),
            args: other_args
                .iter()
                .map(|arg| create_expr(arg.node.clone()))
                .collect(),
            keywords: vec![],
        };
        let expr = replace_by_expr_kind(call, expr, patch)?;
        Ok(Some(expr))
    } else {
        Ok(None)
    }
}

// `expr` => `Expr(expr_kind)`
fn replace_by_expr_kind(node: ExprKind, expr: &Expr, patch: bool) -> Result<Check> {
    let mut check = Check::new(CheckKind::RemoveSixCompat, Range::from_located(expr));
    if patch {
        let mut generator = SourceGenerator::new();
        generator.unparse_expr(&create_expr(node), 0);
        let content = generator.generate()?;
        check.amend(Fix::replacement(
            content,
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    Ok(check)
}

fn replace_by_stmt_kind(node: StmtKind, expr: &Expr, patch: bool) -> Result<Check> {
    let mut check = Check::new(CheckKind::RemoveSixCompat, Range::from_located(expr));
    if patch {
        let mut generator = SourceGenerator::new();
        generator.unparse_stmt(&create_stmt(node));
        let content = generator.generate()?;
        check.amend(Fix::replacement(
            content,
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    Ok(check)
}

// => `raise exc from cause`
fn replace_by_raise_from(
    exc: Option<ExprKind>,
    cause: Option<ExprKind>,
    expr: &Expr,
    patch: bool,
) -> Result<Check> {
    let stmt_kind = StmtKind::Raise {
        exc: exc.map(|exc| Box::new(create_expr(exc))),
        cause: cause.map(|cause| Box::new(create_expr(cause))),
    };
    replace_by_stmt_kind(stmt_kind, expr, patch)
}

fn replace_by_index_on_arg(
    arg: &Expr,
    index: &ExprKind,
    expr: &Expr,
    patch: bool,
) -> Result<Check> {
    let index = ExprKind::Subscript {
        value: Box::new(create_expr(arg.node.clone())),
        slice: Box::new(create_expr(index.clone())),
        ctx: ExprContext::Load,
    };
    replace_by_expr_kind(index, expr, patch)
}

fn handle_reraise(args: &[Expr], expr: &Expr, patch: bool) -> Result<Option<Check>> {
    if let [_, exc, tb] = args {
        let check = replace_by_raise_from(
            Some(ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Attribute {
                    value: Box::new(create_expr(exc.node.clone())),
                    attr: "with_traceback".to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![create_expr(tb.node.clone())],
                keywords: vec![],
            }),
            None,
            expr,
            patch,
        )?;
        Ok(Some(check))
    } else if let [arg] = args {
        if let ExprKind::Starred { value, .. } = &arg.node {
            if let ExprKind::Call { func, .. } = &value.node {
                if let ExprKind::Attribute { value, attr, .. } = &func.node {
                    if let ExprKind::Name { id, .. } = &value.node {
                        if id == "sys" && attr == "exc_info" {
                            let check = replace_by_raise_from(None, None, expr, patch)?;
                            return Ok(Some(check));
                        };
                    };
                };
            };
        };
        Ok(None)
    } else {
        Ok(None)
    }
}

fn handle_func(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    expr: &Expr,
    patch: bool,
    locator: &SourceCodeLocator,
) -> Result<Option<Check>> {
    let func_name = match &func.node {
        ExprKind::Attribute { attr, .. } => attr,
        ExprKind::Name { id, .. } => id,
        _ => bail!("Unexpected func: {:?}", func),
    };
    let check = match (func_name.as_str(), args, keywords) {
        ("b", [arg], []) => replace_by_str_literal(arg, true, expr, patch, locator),
        ("ensure_binary", [arg], []) => replace_by_str_literal(arg, true, expr, patch, locator),
        ("u", [arg], []) => replace_by_str_literal(arg, false, expr, patch, locator),
        ("ensure_str", [arg], []) => replace_by_str_literal(arg, false, expr, patch, locator),
        ("ensure_text", [arg], []) => replace_by_str_literal(arg, false, expr, patch, locator),
        ("iteritems", args, []) => {
            replace_call_on_arg_by_arg_method_call("items", args, expr, patch)?
        }
        ("viewitems", args, []) => {
            replace_call_on_arg_by_arg_method_call("items", args, expr, patch)?
        }
        ("iterkeys", args, []) => {
            replace_call_on_arg_by_arg_method_call("keys", args, expr, patch)?
        }
        ("viewkeys", args, []) => {
            replace_call_on_arg_by_arg_method_call("keys", args, expr, patch)?
        }
        ("itervalues", args, []) => {
            replace_call_on_arg_by_arg_method_call("values", args, expr, patch)?
        }
        ("viewvalues", args, []) => {
            replace_call_on_arg_by_arg_method_call("values", args, expr, patch)?
        }
        ("get_method_function", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__func__", arg, expr, patch,
        )?),
        ("get_method_self", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__self__", arg, expr, patch,
        )?),
        ("get_function_closure", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__closure__",
            arg,
            expr,
            patch,
        )?),
        ("get_function_code", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__code__", arg, expr, patch,
        )?),
        ("get_function_defaults", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__defaults__",
            arg,
            expr,
            patch,
        )?),
        ("get_function_globals", [arg], []) => Some(replace_call_on_arg_by_arg_attribute(
            "__globals__",
            arg,
            expr,
            patch,
        )?),
        ("create_unbound_method", [arg, _], _) => {
            Some(replace_by_expr_kind(arg.node.clone(), expr, patch)?)
        }
        ("get_unbound_function", [arg], []) => {
            Some(replace_by_expr_kind(arg.node.clone(), expr, patch)?)
        }
        ("assertCountEqual", args, []) => {
            replace_call_on_arg_by_arg_method_call("assertCountEqual", args, expr, patch)?
        }
        ("assertRaisesRegex", args, []) => {
            replace_call_on_arg_by_arg_method_call("assertRaisesRegex", args, expr, patch)?
        }
        ("assertRegex", args, []) => {
            replace_call_on_arg_by_arg_method_call("assertRegex", args, expr, patch)?
        }
        ("raise_from", [exc, cause], []) => Some(replace_by_raise_from(
            Some(exc.node.clone()),
            Some(cause.node.clone()),
            expr,
            patch,
        )?),
        ("reraise", args, []) => handle_reraise(args, expr, patch)?,
        ("byte2int", [arg], []) => Some(replace_by_index_on_arg(
            arg,
            &ExprKind::Constant {
                value: Constant::Int(0.into()),
                kind: None,
            },
            expr,
            patch,
        )?),
        ("indexbytes", [arg, index], []) => {
            Some(replace_by_index_on_arg(arg, &index.node, expr, patch)?)
        }
        ("int2byte", [arg], []) => Some(replace_by_expr_kind(
            ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Name {
                    id: "bytes".to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![create_expr(ExprKind::Tuple {
                    elts: vec![create_expr(arg.node.clone())],
                    ctx: ExprContext::Load,
                })],
                keywords: vec![],
            },
            expr,
            patch,
        )?),
        _ => None,
    };
    Ok(check)
}

fn handle_next_on_six_dict(expr: &Expr, patch: bool, checker: &Checker) -> Result<Option<Check>> {
    let ExprKind::Call { func, args, .. } = &expr.node else {
        return Ok(None);
    };
    let ExprKind::Name { id, .. } = &func.node else {
        return Ok(None);
    };
    if id != "next" {
        return Ok(None);
    }
    let [arg] = &args[..] else { return Ok(None); };
    let call_path = dealias_call_path(collect_call_paths(arg), &checker.import_aliases);
    if !is_module_member(&call_path, "six") {
        return Ok(None);
    }
    let ExprKind::Call { func, args, .. } = &arg.node else {return Ok(None);};
    let ExprKind::Attribute { attr, .. } = &func.node else {return Ok(None);};
    let [dict_arg] = &args[..] else {return Ok(None);};
    let method_name = match attr.as_str() {
        "iteritems" => "items",
        "iterkeys" => "keys",
        "itervalues" => "values",
        _ => return Ok(None),
    };
    match replace_by_expr_kind(
        ExprKind::Call {
            func: Box::new(create_expr(ExprKind::Name {
                id: "iter".to_string(),
                ctx: ExprContext::Load,
            })),
            args: vec![create_expr(ExprKind::Call {
                func: Box::new(create_expr(ExprKind::Attribute {
                    value: Box::new(dict_arg.clone()),
                    attr: method_name.to_string(),
                    ctx: ExprContext::Load,
                })),
                args: vec![],
                keywords: vec![],
            })],
            keywords: vec![],
        },
        arg,
        patch,
    ) {
        Ok(check) => Ok(Some(check)),
        Err(err) => Err(err),
    }
}

/// UP016
pub fn remove_six_compat(checker: &mut Checker, expr: &Expr) {
    match handle_next_on_six_dict(expr, checker.patch(&CheckCode::UP016), checker) {
        Ok(Some(check)) => {
            checker.add_check(check);
            return;
        }
        Ok(None) => (),
        Err(err) => {
            error!("Error while removing `six` reference: {}", err);
            return;
        }
    };
    let call_path = dealias_call_path(collect_call_paths(expr), &checker.import_aliases);
    if is_module_member(&call_path, "six") {
        let patch = checker.patch(&CheckCode::UP016);
        let check = match &expr.node {
            ExprKind::Call {
                func,
                args,
                keywords,
            } => match handle_func(func, args, keywords, expr, patch, checker.locator) {
                Ok(check) => check,
                Err(err) => {
                    error!("Failed to remove `six` reference: {err}");
                    return;
                }
            },
            ExprKind::Attribute { attr, .. } => map_name(attr.as_str(), expr, patch),
            ExprKind::Name { id, .. } => map_name(id.as_str(), expr, patch),
            _ => return,
        };
        if let Some(check) = check {
            checker.add_check(check);
        }
    }
}
